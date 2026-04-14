use crate::config::{AssistLocationConfig, AssistLocationPageConfig};
use crate::models::{Coordinate, AssistLocation, ProcessedImage};
use crate::myutils::image::{merge_coordinates, integral_image, sum_pixel};
use crate::recognize::align::{
    extract_y_centers, find_missing_indices_cos, find_extra_indices_cos, filter_by_extra_indices,
};
use anyhow::{Ok, Result};
use image::GrayImage;
use imageproc::contours::find_contours;

pub struct AssistLocationModule;

impl AssistLocationModule {
    pub fn new() -> Self {
        Self
    }

    pub fn infer_single<T: AssistLocationConfig>(
        &self,
        processed_image: &ProcessedImage,
        assist_location: &mut AssistLocation,
    ) -> Result<AssistLocation> {
        let left_area = merge_coordinates(
            &assist_location.left,
            T::assist_area_extend_size_w(),
            T::assist_area_extend_size_h(),
        );
        let right_area = merge_coordinates(
            &assist_location.right,
            T::assist_area_extend_size_w(),
            T::assist_area_extend_size_h(),
        );
        let left_src_assist = Self::find_assist_location::<T>(&processed_image.closed, &left_area)?;
        let right_src_assist = Self::find_assist_location::<T>(&processed_image.closed, &right_area)?;

        // 允许的最大多检/漏检数量
        const MAX_DIFF: usize = 2;

        let expected_count = assist_location.left.len(); // 左右标注数量应该相同
        let detected_left_count = left_src_assist.len();
        let detected_right_count = right_src_assist.len();

        let left_match = detected_left_count == expected_count;
        let right_match = detected_right_count == expected_count;

        // 如果两列都完全匹配，直接返回
        if left_match && right_match {
            return Ok(AssistLocation {
                left: left_src_assist,
                right: right_src_assist,
            });
        }

        // 必须有一列完全匹配才能进入对齐逻辑
        if !left_match && !right_match {
            anyhow::bail!(
                "左右两列都不匹配，无法对齐。左侧检测{}个/期望{}个，右侧检测{}个/期望{}个",
                detected_left_count,
                expected_count,
                detected_right_count,
                expected_count
            );
        }

        // 检查不匹配的那列差异是否在允许范围内
        let diff = if !left_match {
            (detected_left_count as i32 - expected_count as i32).abs() as usize
        } else {
            (detected_right_count as i32 - expected_count as i32).abs() as usize
        };

        if diff > MAX_DIFF {
            anyhow::bail!(
                "辅助定位点数量差异过大（最大允许{}），左侧检测{}个/期望{}个，右侧检测{}个/期望{}个",
                MAX_DIFF,
                detected_left_count,
                expected_count,
                detected_right_count,
                expected_count
            );
        }

        // 对不匹配的列进行处理
        if left_match {
            // 左侧是完整列，右侧需要处理
            let left_y = extract_y_centers(&left_src_assist);
            let right_y = extract_y_centers(&right_src_assist);

            if detected_right_count < expected_count {
                // 右侧漏检：找出缺失的标注点索引，从右侧标注中删除
                let missing_indices = find_missing_indices_cos(&left_y, &right_y);

                // 从后往前删除标注数据中的右侧点
                let mut indices_to_remove = missing_indices.clone();
                indices_to_remove.sort_by(|a, b| b.cmp(a));
                for idx in indices_to_remove {
                    assist_location.right.remove(idx);
                }

                Ok(AssistLocation {
                    left: left_src_assist,
                    right: right_src_assist,
                })
            } else {
                // 右侧多检：找出多余的检测点索引，过滤掉
                let extra_indices = find_extra_indices_cos(&left_y, &right_y);

                let final_right = filter_by_extra_indices(&right_src_assist, &extra_indices);

                Ok(AssistLocation {
                    left: left_src_assist,
                    right: final_right,
                })
            }
        } else {
            // 右侧是完整列，左侧需要处理
            let right_y = extract_y_centers(&right_src_assist);
            let left_y = extract_y_centers(&left_src_assist);

            if detected_left_count < expected_count {
                // 左侧漏检：找出缺失的标注点索引，从左侧标注中删除
                let missing_indices = find_missing_indices_cos(&right_y, &left_y);

                // 从后往前删除标注数据中的左侧点
                let mut indices_to_remove = missing_indices.clone();
                indices_to_remove.sort_by(|a, b| b.cmp(a));
                for idx in indices_to_remove {
                    assist_location.left.remove(idx);
                }

                Ok(AssistLocation {
                    left: left_src_assist,
                    right: right_src_assist,
                })
            } else {
                // 左侧多检：找出多余的检测点索引，过滤掉
                let extra_indices = find_extra_indices_cos(&right_y, &left_y);

                let final_left = filter_by_extra_indices(&left_src_assist, &extra_indices);

                Ok(AssistLocation {
                    left: final_left,
                    right: right_src_assist,
                })
            }
        }
    }

    // 求所有coor的x中位数，过滤掉x远离中位数的coor
    fn filter_assist_location_by_x(assist_location: &Vec<Coordinate>) -> Vec<Coordinate> {
        if assist_location.is_empty() {
            return Vec::new();
        }

        // 提取所有x坐标
        let mut x_values: Vec<i32> = assist_location.iter().map(|c| c.x).collect();

        // 计算中位数
        x_values.sort_unstable();
        let median = if x_values.len() % 2 == 0 {
            let mid = x_values.len() / 2;
            (x_values[mid - 1] + x_values[mid]) / 2
        } else {
            x_values[x_values.len() / 2]
        };

        let threshold = AssistLocationPageConfig::assist_point_x_median_diff();

        // 过滤掉x值远离中位数的坐标
        assist_location
            .iter()
            .filter(|c| (c.x - median).abs() <= threshold)
            .cloned()
            .collect()
    }

    pub fn infer_paper(
        &self,
        processed_image: &ProcessedImage,
        assist_location: &mut AssistLocation,
    ) -> Result<AssistLocation> {
        let mut assist_locations = Vec::new();
        let mut split_locations = assist_location.split();
        for single_location in split_locations.iter_mut() {
            let real_single_location =
                self.infer_single::<AssistLocationPageConfig>(processed_image, single_location)?;
            assist_locations.push(real_single_location);
        }
        let res = AssistLocation::merge(&assist_locations);

        // 把修改后的 split_locations 合并回 assist_location
        let merged_ref = AssistLocation::merge(&split_locations);
        assist_location.left = merged_ref.left;
        assist_location.right = merged_ref.right;

        Ok(res)
    }

    /// 在闭图上寻找辅助定位点
    pub fn find_assist_location<T: AssistLocationConfig>(
        closed: &GrayImage,
        coordinate: &Coordinate,
    ) -> Result<Vec<Coordinate>> {
        // 裁剪ROI区域
        let roi_x = coordinate.x.max(0) as u32;
        let roi_y = coordinate.y.max(0) as u32;
        let roi_w = coordinate
            .w
            .min(closed.width() as i32 - coordinate.x.max(0)) as u32;
        let roi_h = coordinate
            .h
            .min(closed.height() as i32 - coordinate.y.max(0)) as u32;

        let roi = image::imageops::crop_imm(closed, roi_x, roi_y, roi_w, roi_h).to_image();

        // 查找轮廓
        let contours = find_contours::<u32>(&roi);

        let mut assist_points = Vec::new();
        let integral_image = integral_image(&roi)?;

        // 遍历所有轮廓
        for contour in contours {
            // 计算轮廓的边界矩形
            let (min_x, min_y, max_x, max_y) = get_contour_bounds(&contour.points);
            let width = max_x - min_x;
            let height = max_y - min_y;
            let area = (width * height) as f64;

            if width < T::assist_point_min_size() {
                continue;
            }
            if width > T::assist_point_max_size() {
                continue;
            }
            if height < T::assist_point_min_size() {
                continue;
            }
            if height > T::assist_point_max_size() {
                continue;
            }
            if (width - height).abs() > T::assist_point_whdiff_max() {
                continue;
            }
            if area < T::assist_point_min_area() {
                continue;
            }
            if area > T::assist_point_max_area() {
                continue;
            }

            let fill_rate = calculate_fill_rate(
                &integral_image,
                &Coordinate {
                    x: min_x + 1,
                    y: min_y + 1,
                    w: width - 2,
                    h: height - 2,
                },
            )?;

            if fill_rate < T::assist_point_min_fill_ratio() {
                continue;
            }

            assist_points.push(Coordinate {
                x: min_x + coordinate.x,
                y: min_y + coordinate.y,
                w: width,
                h: height,
            });
        }

        assist_points.sort_by(|a, b| a.y.cmp(&b.y));

        let assist_points = Self::filter_assist_location_by_x(&assist_points);

        Ok(assist_points)
    }
}

/// 计算轮廓的边界框
fn get_contour_bounds(points: &[imageproc::point::Point<u32>]) -> (i32, i32, i32, i32) {
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

/// 计算指定区域的填涂率（白色像素占比）
fn calculate_fill_rate(integral: &Vec<Vec<u64>>, coordinate: &Coordinate) -> Result<f64> {
    // 获取积分图尺寸
    let integral_rows = integral.len() as i32;
    let integral_cols = integral[0].len() as i32;

    // 检查坐标是否有效
    if coordinate.x < 0
        || coordinate.y < 0
        || coordinate.x + coordinate.w > integral_cols - 1
        || coordinate.y + coordinate.h > integral_rows - 1
    {
        anyhow::bail!("坐标超出积分图范围");
    }

    let sum = sum_pixel(integral, coordinate);

    // 计算区域面积
    let area = coordinate.w as f64 * coordinate.h as f64;

    // 计算白色像素占比（填涂率）
    // 由于二值图中白色为255，黑色为0，所以需要将和除以255得到白色像素数量
    let white_pixels = sum as f64 / 255.0;
    let fill_rate = white_pixels / area;

    Ok(fill_rate)
}
