use anyhow::Result;
use crate::models::{ProcessedImage, AssistLocation, Coordinate};
use crate::config::AssistLocationConfig;
use imageproc::contours::find_contours;

pub struct AssistLocationModule;

impl AssistLocationModule {
    pub fn new() -> Self {
        Self
    }

    pub fn infer_paper<T: AssistLocationConfig>(
        &self,
        processed_image: &ProcessedImage,
        mark_assist_location: &mut AssistLocation,
    ) -> Result<AssistLocation> {
        // 在左侧和右侧区域分别查找辅助定位点
        let left_points = self.find_assist_points::<T>(
            &processed_image.closed,
            &mark_assist_location.left,
        )?;

        let right_points = self.find_assist_points::<T>(
            &processed_image.closed,
            &mark_assist_location.right,
        )?;

        Ok(AssistLocation {
            left: left_points,
            right: right_points,
        })
    }

    fn find_assist_points<T: AssistLocationConfig>(
        &self,
        image: &image::GrayImage,
        expected_points: &Vec<Coordinate>,
    ) -> Result<Vec<Coordinate>> {
        let mut found_points = Vec::new();

        for expected in expected_points {
            // 扩展搜索区域
            let search_x = (expected.x - T::assist_area_extend_size_w()).max(0);
            let search_y = (expected.y - T::assist_area_extend_size_h()).max(0);
            let search_w = expected.w + T::assist_area_extend_size_w() * 2;
            let search_h = expected.h + T::assist_area_extend_size_h() * 2;

            // 裁剪搜索区域
            let search_region = image::imageops::crop_imm(
                image,
                search_x as u32,
                search_y as u32,
                search_w.min(image.width() as i32 - search_x) as u32,
                search_h.min(image.height() as i32 - search_y) as u32,
            ).to_image();

            // 在搜索区域内查找轮廓
            let contours = find_contours::<u32>(&search_region);

            let mut best_point = None;
            let mut best_score = f64::NEG_INFINITY;

            for contour in contours {
                // 计算轮廓的边界框
                let (min_x, min_y, max_x, max_y) = self.get_contour_bounds(&contour.points);
                let w = max_x - min_x;
                let h = max_y - min_y;

                // 检查尺寸
                if w < T::assist_point_min_size() || w > T::assist_point_max_size() ||
                   h < T::assist_point_min_size() || h > T::assist_point_max_size() {
                    continue;
                }

                // 检查宽高差
                if (w - h).abs() > T::assist_point_whdiff_max() {
                    continue;
                }

                // 计算面积
                let area = (w * h) as f64;
                if area < T::assist_point_min_area() || area > T::assist_point_max_area() {
                    continue;
                }

                // 计算填充率
                let contour_area = contour.points.len() as f64;
                let fill_ratio = contour_area / area;
                if fill_ratio < T::assist_point_min_fill_ratio() {
                    continue;
                }

                // 计算与期望位置的距离
                let center_x = (min_x + max_x) / 2 + search_x;
                let center_y = (min_y + max_y) / 2 + search_y;
                let expected_center_x = expected.x + expected.w / 2;
                let expected_center_y = expected.y + expected.h / 2;
                let distance = (((center_x - expected_center_x).pow(2) + (center_y - expected_center_y).pow(2)) as f64).sqrt();

                // 得分：距离越近越好
                let score = -distance;

                if score > best_score {
                    best_score = score;
                    best_point = Some(Coordinate {
                        x: min_x + search_x,
                        y: min_y + search_y,
                        w,
                        h,
                    });
                }
            }

            // 如果找到了点，使用找到的点；否则使用期望的点
            if let Some(point) = best_point {
                found_points.push(point);
            } else {
                found_points.push(expected.clone());
            }
        }

        Ok(found_points)
    }

    fn get_contour_bounds(&self, points: &[imageproc::point::Point<u32>]) -> (i32, i32, i32, i32) {
        let mut min_x = u32::MAX;
        let mut min_y = u32::MAX;
        let mut max_x = 0u32;
        let mut max_y = 0u32;

        for point in points {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }

        (min_x as i32, min_y as i32, max_x as i32, max_y as i32)
    }
}
