use anyhow::Result;
use crate::models::{ProcessedImage, Quad};
use crate::config::ImageProcessingConfig;
use imageproc::contours::find_contours;
use imageproc::geometry::{approximate_polygon_dp, contour_area, arc_length};

pub struct LocationModule;

impl LocationModule {
    pub fn new() -> Self {
        Self
    }

    pub fn infer(&self, processed_image: &ProcessedImage) -> Result<Quad> {
        // 使用 closed_for_location 进行轮廓检测
        let contours = find_contours::<u32>(&processed_image.closed_for_location);

        let (width, height) = (
            processed_image.closed_for_location.width() as f64,
            processed_image.closed_for_location.height() as f64,
        );
        let min_area = ImageProcessingConfig::MIN_AREA_RATIO * width * height;

        let mut best_contour = None;
        let mut best_score = f64::NEG_INFINITY;

        for contour in contours {
            // 使用 imageproc 的优化实现计算轮廓面积
            let area = contour_area(&contour.points);

            if area < min_area {
                continue;
            }

            // 使用 imageproc 的优化实现计算周长
            let perimeter = arc_length(&contour.points, true);

            // 多边形逼近
            let epsilon = ImageProcessingConfig::EPSILON_FACTOR * perimeter;

            // 将 u32 点转换为 i32 点
            let points_i32: Vec<imageproc::point::Point<i32>> = contour.points.iter()
                .map(|p| imageproc::point::Point::new(p.x as i32, p.y as i32))
                .collect();

            let approx = approximate_polygon_dp(&points_i32, epsilon, true);

            // 只保留四边形
            if approx.len() != 4 {
                continue;
            }

            // 计算边界框
            let (min_x, min_y, max_x, max_y) = get_bounding_box(&approx);
            let margin = min_x.min(min_y).min(width as i32 - max_x).min(height as i32 - max_y).max(0);

            // 计算得分：面积越大越好，边距越小越好
            let score = area - (margin as f64) * ImageProcessingConfig::MARGIN_PENALTY;

            if score > best_score {
                best_score = score;
                best_contour = Some(approx);
            }
        }

        if let Some(contour) = best_contour {
            // 确保点的顺序：左上、右上、右下、左下
            let sorted_points = sort_quad_points(&contour);
            Ok(Quad {
                points: [
                    (sorted_points[0].x, sorted_points[0].y),
                    (sorted_points[1].x, sorted_points[1].y),
                    (sorted_points[2].x, sorted_points[2].y),
                    (sorted_points[3].x, sorted_points[3].y),
                ],
            })
        } else {
            // 如果没有找到合适的轮廓，返回图像边缘的四边形
            let margin = 50;
            Ok(Quad {
                points: [
                    (margin, margin),
                    (width as i32 - margin, margin),
                    (width as i32 - margin, height as i32 - margin),
                    (margin, height as i32 - margin),
                ],
            })
        }
    }
}

/// 获取边界框
fn get_bounding_box(points: &[imageproc::point::Point<i32>]) -> (i32, i32, i32, i32) {
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for point in points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }

    (min_x, min_y, max_x, max_y)
}

/// 对四边形的点进行排序：左上、右上、右下、左下
fn sort_quad_points(points: &[imageproc::point::Point<i32>]) -> Vec<imageproc::point::Point<i32>> {
    if points.len() != 4 {
        return points.to_vec();
    }

    let mut sorted = points.to_vec();

    // 计算中心点
    let center_x = sorted.iter().map(|p| p.x).sum::<i32>() as f64 / 4.0;
    let center_y = sorted.iter().map(|p| p.y).sum::<i32>() as f64 / 4.0;

    // 根据角度排序
    sorted.sort_by(|a, b| {
        let angle_a = ((a.y as f64 - center_y).atan2(a.x as f64 - center_x) * 180.0 / std::f64::consts::PI + 360.0) % 360.0;
        let angle_b = ((b.y as f64 - center_y).atan2(b.x as f64 - center_x) * 180.0 / std::f64::consts::PI + 360.0) % 360.0;
        angle_a.partial_cmp(&angle_b).unwrap()
    });

    // 找到左上角的点（x+y最小）
    let mut min_sum = i32::MAX;
    let mut start_idx = 0;
    for (i, point) in sorted.iter().enumerate() {
        let sum = point.x + point.y;
        if sum < min_sum {
            min_sum = sum;
            start_idx = i;
        }
    }

    // 从左上角开始重新排列
    let mut result = Vec::new();
    for i in 0..4 {
        result.push(sorted[(start_idx + i) % 4]);
    }

    result
}
