use anyhow::Result;
use crate::models::{ContourInfo, Quad, ProcessedImage};
use crate::config::ImageProcessingConfig;
use crate::myutils::math::{avg_distance_contour_to_quad, point_to_quad_min_distance, distance_point2i};
use imageproc::contours::find_contours;
use imageproc::geometry::{approximate_polygon_dp, contour_area, arc_length};

pub struct LocationModule;

impl LocationModule {
    pub fn new() -> Self {
        Self
    }

    pub fn infer(&self, processed_image: &ProcessedImage) -> Result<Quad> {
        let mut boundaries = self.detect_boundary(&processed_image.closed)?;
        let best_idx = self.select_best_contour_index(&boundaries, &processed_image.closed)?;
        let boundary = self.contour_to_quad(&mut boundaries[best_idx])?;
        let valid = self.validate_boundary(&boundary, &boundaries[best_idx]);
        if !valid {
            anyhow::bail!("需要铺平试卷!");
        }
        Ok(boundary)
    }

    pub fn detect_boundary(&self, morphology: &image::GrayImage) -> Result<Vec<ContourInfo>> {
        // 查找连通区域（外部轮廓）
        let contours = find_contours::<u32>(morphology);

        let h = morphology.height() as f64;
        let w = morphology.width() as f64;
        let min_area = ImageProcessingConfig::MIN_AREA_RATIO * w * h;

        let mut contour_infos = Vec::new();
        for contour in contours {
            let area = contour_area(&contour.points);

            // 第一步：只做最小面积占比的筛选
            if area < min_area {
                continue;
            }

            // 将 u32 点转换为 i32 点
            let points: Vec<(i32, i32)> = contour
                .points
                .iter()
                .map(|p| (p.x as i32, p.y as i32))
                .collect();

            contour_infos.push(ContourInfo { points, area });
        }

        Ok(contour_infos)
    }

    /// 从多个候选轮廓中选择最合适的一个，返回索引
    fn select_best_contour_index(
        &self,
        boundaries: &Vec<ContourInfo>,
        image: &image::GrayImage,
    ) -> Result<usize> {
        if boundaries.is_empty() {
            anyhow::bail!("未找到合适的外部黑框");
        }

        let mut best_index = 0;
        let mut best_score = f64::NEG_INFINITY;

        let w = image.width() as i32;
        let h = image.height() as i32;

        for (idx, contour_info) in boundaries.iter().enumerate() {
            let area = contour_info.area;
            let bounding_rect = get_bounding_rect(&contour_info.points);
            let x = bounding_rect.0;
            let y = bounding_rect.1;
            let cw = bounding_rect.2;
            let ch = bounding_rect.3;
            let margin = x.min(y).min(w - x - cw).min(h - y - ch).max(0);
            let score = area - (margin as f64) * ImageProcessingConfig::MARGIN_PENALTY;

            if score > best_score {
                best_score = score;
                best_index = idx;
            }
        }

        Ok(best_index)
    }

    /// 将轮廓信息转换为四边形
    pub fn contour_to_quad(&self, contour_info: &mut ContourInfo) -> Result<Quad> {
        // 将 i32 点转换为 imageproc 的 Point<i32>
        let points_imageproc: Vec<imageproc::point::Point<i32>> = contour_info
            .points
            .iter()
            .map(|&(x, y)| imageproc::point::Point::new(x, y))
            .collect();

        // 计算周长
        let perimeter = arc_length(&points_imageproc, true);

        // 使用轮廓近似算法提取四边形
        let epsilon = ImageProcessingConfig::EPSILON_FACTOR * perimeter;
        let mut approx_curve = approximate_polygon_dp(&points_imageproc, epsilon, true);

        // 如果点数不是4，使用凸包作为备选方案
        if approx_curve.len() != 4 {
            let hull = convex_hull(&points_imageproc);

            // 对凸包进行多边形逼近
            approx_curve = approximate_polygon_dp(&hull, epsilon, true);

            if approx_curve.len() != 4 {
                if approx_curve.len() > 4 && approx_curve.len() <= 6 {
                    // 从多个候选点中选出最优的4个点
                    let quad = self.select_best_quad_from_points(&approx_curve, contour_info)?;
                    return Ok(quad);
                }
                anyhow::bail!(
                    "未能找到合适的四边形，原始轮廓顶点数: {}，凸包逼近后顶点数: {}",
                    contour_info.points.len(),
                    approx_curve.len()
                );
            }
        }

        // 提取四个点
        let mut points_array: [(i32, i32); 4] = [
            (approx_curve[0].x, approx_curve[0].y),
            (approx_curve[1].x, approx_curve[1].y),
            (approx_curve[2].x, approx_curve[2].y),
            (approx_curve[3].x, approx_curve[3].y),
        ];

        // 确保四个点按顺时针方向排列，从左上角开始
        Self::order_points(&mut points_array);

        Ok(Quad {
            points: points_array,
        })
    }

    /// 从多个候选点中选出最优的4个点构成四边形
    /// 选择标准：轮廓点到四边形边的平均最近距离最小
    /// 约束：任意两个选中点之间的距离不小于 MIN_POINT_DISTANCE
    const MIN_POINT_DISTANCE: f64 = 800.0;

    fn select_best_quad_from_points(
        &self,
        candidates: &[imageproc::point::Point<i32>],
        contour_info: &mut ContourInfo,
    ) -> Result<Quad> {
        let n = candidates.len();
        let points: Vec<(i32, i32)> = candidates.iter().map(|p| (p.x, p.y)).collect();

        let mut best_avg_dist = f64::MAX;
        let mut best_quad: Option<[(i32, i32); 4]> = None;
        let mut best_indices: Option<[usize; 4]> = None;

        // 枚举所有 C(n,4) 的组合
        for i in 0..n {
            for j in (i + 1)..n {
                if distance_point2i(&points[i], &points[j]) < Self::MIN_POINT_DISTANCE {
                    continue;
                }
                for k in (j + 1)..n {
                    if distance_point2i(&points[i], &points[k]) < Self::MIN_POINT_DISTANCE
                        || distance_point2i(&points[j], &points[k]) < Self::MIN_POINT_DISTANCE
                    {
                        continue;
                    }
                    for l in (k + 1)..n {
                        if distance_point2i(&points[i], &points[l]) < Self::MIN_POINT_DISTANCE
                            || distance_point2i(&points[j], &points[l]) < Self::MIN_POINT_DISTANCE
                            || distance_point2i(&points[k], &points[l]) < Self::MIN_POINT_DISTANCE
                        {
                            continue;
                        }

                        let mut pts = [points[i], points[j], points[k], points[l]];
                        Self::order_points(&mut pts);
                        let quad = Quad { points: pts };

                        let avg_dist = avg_distance_contour_to_quad(&quad, contour_info);
                        if avg_dist < best_avg_dist {
                            best_avg_dist = avg_dist;
                            best_quad = Some(pts);
                            best_indices = Some([i, j, k, l]);
                        }
                    }
                }
            }
        }

        match (best_quad, best_indices) {
            (Some(pts), Some(selected)) => {
                let quad = Quad { points: pts };

                // 找出未被选中的离群点，从 contour_info.points 中移除其附近的轮廓点
                let selected_set: std::collections::HashSet<usize> =
                    selected.iter().cloned().collect();
                for idx in 0..n {
                    if selected_set.contains(&idx) {
                        continue;
                    }
                    // 离群点到 quad 最近边的距离作为清除半径
                    let outlier = &points[idx];
                    let radius = point_to_quad_min_distance(*outlier, &quad);

                    // 从 contour_info.points 中删除距离该离群点 < radius 的点
                    contour_info.points.retain(|p| distance_point2i(p, outlier) >= radius);
                }

                Ok(quad)
            }
            _ => anyhow::bail!(
                "无法从{}个候选点中选出满足最小距离约束的四边形",
                n
            ),
        }
    }

    /// 对四边形的四个顶点进行排序，确保按顺时针方向排列，从左上角开始
    pub fn order_points(pts: &mut [(i32, i32); 4]) {
        // 计算质心
        let centroid_x = (pts[0].0 + pts[1].0 + pts[2].0 + pts[3].0) as f32 / 4.0;
        let centroid_y = (pts[0].1 + pts[1].1 + pts[2].1 + pts[3].1) as f32 / 4.0;

        // 按角度排序
        pts.sort_by(|a, b| {
            let angle_a = (a.1 as f32 - centroid_y).atan2(a.0 as f32 - centroid_x);
            let angle_b = (b.1 as f32 - centroid_y).atan2(b.0 as f32 - centroid_x);

            angle_a.partial_cmp(&angle_b).unwrap()
        });

        // 确保第一个点是左上角（x和y值最小的点）
        let mut min_index = 0;
        let mut min_sum = pts[0].0 + pts[0].1;

        for i in 1..4 {
            let sum = pts[i].0 + pts[i].1;
            if sum < min_sum {
                min_sum = sum;
                min_index = i;
            }
        }

        // 旋转数组，使左上角点成为第一个点
        pts.rotate_left(min_index);
    }

    // 以boundary构建四边形，计算contour上所有点到最近边的距离，对所有距离求平均值，判断平均距离是否在合理范围内
    pub fn validate_boundary(&self, boundary: &Quad, contour: &ContourInfo) -> bool {
        let avg_distance = avg_distance_contour_to_quad(boundary, contour);

        let threshold = ImageProcessingConfig::BOUNDARY_PENALTY;

        avg_distance <= threshold
    }
}

/// 获取边界矩形 (x, y, width, height)
fn get_bounding_rect(points: &[(i32, i32)]) -> (i32, i32, i32, i32) {
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    (min_x, min_y, max_x - min_x, max_y - min_y)
}

/// 计算凸包
fn convex_hull(points: &[imageproc::point::Point<i32>]) -> Vec<imageproc::point::Point<i32>> {
    if points.len() < 3 {
        return points.to_vec();
    }

    let mut pts = points.to_vec();
    pts.sort_by(|a, b| {
        if a.x != b.x {
            a.x.cmp(&b.x)
        } else {
            a.y.cmp(&b.y)
        }
    });

    // 计算叉积
    let cross = |o: &imageproc::point::Point<i32>,
                 a: &imageproc::point::Point<i32>,
                 b: &imageproc::point::Point<i32>|
     -> i64 {
        ((a.x - o.x) as i64) * ((b.y - o.y) as i64)
            - ((a.y - o.y) as i64) * ((b.x - o.x) as i64)
    };

    // 构建下凸包
    let mut lower = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(&lower[lower.len() - 2], &lower[lower.len() - 1], p) <= 0 {
            lower.pop();
        }
        lower.push(*p);
    }

    // 构建上凸包
    let mut upper = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(&upper[upper.len() - 2], &upper[upper.len() - 1], p) <= 0 {
            upper.pop();
        }
        upper.push(*p);
    }

    // 移除最后一个点，因为它与另一个凸包的第一个点重复
    lower.pop();
    upper.pop();

    lower.extend(upper);
    lower
}
