use anyhow::Result;
use crate::models::{Coordinate, ProcessedImage, MobileOutput, RecType, RecOption};
use crate::config::FillConfig;
use crate::myutils::image::{integral_image, sum_pixel};
use image::imageops::colorops::contrast_in_place;

pub struct RecFillModule;

impl RecFillModule {
    pub fn new() -> Self {
        Self
    }

    pub fn infer<T: FillConfig>(&self, process_image: &ProcessedImage, mobile_output: &mut MobileOutput) -> Result<()> {
        // 0. 增强对比度并反转灰度图（因为线条为黑色，背景为白色，而我们计算填涂率时需要白色像素数量）
        let mut inverted_gray = process_image.gray.clone();
        // 增强对比度，正值增加对比度，使铅笔痕迹更明显
        contrast_in_place(&mut inverted_gray, T::gray_contrast_enhance());
        // debug 打印增加对比度后的图
        #[cfg(debug_assertions)]
        {
            let debug_path = "dev/test_data/debug/z_contrast_gray.jpg";
            let _ = inverted_gray.save(debug_path);
        }
        image::imageops::invert(&mut inverted_gray);

        // 1. 计算积分图
        let integral = integral_image(&inverted_gray)?;

        // 2. 计算所有选项的填涂率和otsu值
        self.refine_all_fill_coordinate::<T>(&integral, mobile_output)?;
        self.calculate_all_fill_rate(&integral, mobile_output)?;

        let fill_rates: Vec<f64> = mobile_output.rec_results.iter()
            .filter(|rec_result| [RecType::SingleChoice, RecType::MultipleChoice].contains(&rec_result.rec_type))
            .flat_map(|rec_result| rec_result.rec_options.iter().map(|item| item.fill_rate))
            .collect();

        let (mut thresh, _) = crate::myutils::math::otsu_threshold(&fill_rates);
        thresh = thresh.max(T::fill_rate_min());
        thresh = (thresh * 100.0).ceil() / 100.0;

        #[cfg(debug_assertions)]
        {
            println!("填涂率阈值: {:.4}", thresh);
        }

        // 3. 设置填涂结果
        self.set_default_fill(mobile_output, thresh)?;

        Ok(())
    }

    pub fn set_default_fill(&self, mobile_output: &mut MobileOutput, thresh: f64) -> Result<()> {
        for rec_result in mobile_output.rec_results.iter_mut() {
            if ![RecType::SingleChoice, RecType::MultipleChoice].contains(&rec_result.rec_type) {
                continue;
            }
            let fill_items = &mut rec_result.rec_options;
            for (index, fill_item) in fill_items.iter_mut().enumerate() {
                if fill_item.fill_rate > thresh {
                    rec_result.rec_result[index] = true;
                } else {
                    rec_result.rec_result[index] = false;
                }
            }
        }

        Ok(())
    }

    pub fn calculate_all_fill_rate(&self, integral: &Vec<Vec<u64>>, mobile_output: &mut MobileOutput) -> Result<()> {
        for rec_result in mobile_output.rec_results.iter_mut() {
            if ![RecType::SingleChoice, RecType::MultipleChoice].contains(&rec_result.rec_type) {
                continue;
            }
            let fill_items = &mut rec_result.rec_options;
            for fill_item in fill_items.iter_mut() {
                let fill_rate = calculate_fill_rate(integral, &fill_item.coordinate)?;
                fill_item.fill_rate = fill_rate;
            }
        }

        Ok(())
    }

    pub fn refine_all_fill_coordinate<T: FillConfig>(&self, integral: &Vec<Vec<u64>>, mobile_output: &mut MobileOutput) -> Result<()> {
        for rec_result in mobile_output.rec_results.iter_mut() {
            if ![RecType::SingleChoice, RecType::MultipleChoice].contains(&rec_result.rec_type) {
                continue;
            }
            let res = self.refine_items_fill_coordinate::<T>(integral, &mut rec_result.rec_options);
            if res.is_err() {
                continue;
            }
        }

        Ok(())
    }

    /// 通过Otsu最大类间方差优化坐标位置
    /// 在以当前坐标为中心的4x4范围内(-2到2)寻找使所有选项填涂率方差最大的位置
    fn refine_items_fill_coordinate<T: FillConfig>(&self, integral: &Vec<Vec<u64>>, fill_items: &mut Vec<RecOption>) -> Result<()> {
        if fill_items.is_empty() {
            return Ok(());
        }

        let mut max_variance = 0.0;
        let mut best_coordinates: Vec<Coordinate> = Vec::new();

        // 在-2到2的范围内搜索最优坐标偏移
        'outer: for dx in -T::refine_coor_range() ..= T::refine_coor_range() {
            for dy in -T::refine_coor_range() ..= T::refine_coor_range() {
                let mut fill_rates = Vec::new();
                let mut temp_coordinates = Vec::new();

                // 计算所有选项在这个偏移下的填涂率
                for fill_item in fill_items.iter() {
                    let new_coordinate = Coordinate {
                        x: fill_item.coordinate.x + dx,
                        y: fill_item.coordinate.y + dy,
                        w: fill_item.coordinate.w,
                        h: fill_item.coordinate.h,
                    };

                    // 计算填涂率并处理可能的错误
                    let fill_rate_result = calculate_fill_rate(integral, &new_coordinate)?;
                    fill_rates.push(fill_rate_result);
                    temp_coordinates.push(new_coordinate);
                }
                // 如果所有fill_rate都大于0.8，结束搜索
                if fill_rates.iter().all(|&rate| rate > 0.8) {
                    best_coordinates = temp_coordinates;
                    max_variance = f64::MAX;
                    break 'outer;
                }

                let (_, variance) = crate::myutils::math::otsu_threshold(&fill_rates);
                // 更新最优坐标（如果方差更大）
                if variance > max_variance {
                    max_variance = variance;
                    best_coordinates = temp_coordinates;
                }
            }
        }

        // 如果找到了更好的坐标，则更新坐标
        if max_variance > 0.0 {
            for (i, fill_item) in fill_items.iter_mut().enumerate() {
                fill_item.coordinate = best_coordinates[i].clone();
            }
        }

        Ok(())
    }
}

/// 计算指定区域的填涂率（白色像素占比）
pub fn calculate_fill_rate(integral: &Vec<Vec<u64>>, coordinate: &Coordinate) -> Result<f64> {
    // 获取积分图尺寸
    let integral_rows = integral.len() as i32;
    let integral_cols = if integral_rows > 0 { integral[0].len() as i32 } else { 0 };

    // 检查坐标是否有效
    if coordinate.x < 0 || coordinate.y < 0 ||
        coordinate.x + coordinate.w > integral_cols - 1 ||
        coordinate.y + coordinate.h > integral_rows - 1 {
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
