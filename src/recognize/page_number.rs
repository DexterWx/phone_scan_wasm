use anyhow::Result;
use crate::models::{ProcessedImage, Coordinate};
use crate::config::CommonConfig;
use crate::myutils::image::{integral_image, sum_pixel};
use crate::myutils::math::otsu_threshold;

pub struct PageNumberModule;

impl PageNumberModule {
    pub fn new() -> Self {
        Self
    }

    pub fn infer(&self, processed_image: &ProcessedImage, page_numbers: &Vec<Coordinate>) -> Result<usize> {
        if page_numbers.is_empty() {
            return Ok(1);
        }

        // 计算积分图
        let integral = integral_image(&processed_image.thresh)?;

        // 精炼页码坐标
        let refine_coors = self.refine_page_number_coor(&integral, page_numbers)?;

        // 构建二进制字符串
        let mut binary_str = String::new();

        for (idx, coord) in refine_coors.iter().enumerate() {
            // 跳过第一个坐标（索引0）
            if idx == 0 {
                continue;
            }

            let fill_rate = calculate_fill_rate(&integral, coord)?;

            if fill_rate >= CommonConfig::PAGE_NUMBER_FILL_RATE {
                binary_str.push('1');
            } else {
                binary_str.push('0');
            }
        }

        // 将二进制字符串转换为十进制
        match u32::from_str_radix(&binary_str, 2) {
            Ok(decimal) => {
                if decimal == 0 {
                    anyhow::bail!("页码点异常");
                }
                Ok(decimal as usize)
            }
            Err(e) => {
                anyhow::bail!("页码转换失败: {}", e);
            }
        }
    }

    fn refine_page_number_coor(&self, integral: &Vec<Vec<u64>>, coors: &Vec<Coordinate>) -> Result<Vec<Coordinate>> {
        let mut refined_coors = coors.clone();
        let mut max_var = 0.0;

        for move_y in -CommonConfig::PAGE_NUMBER_EXTEND_SIZE..CommonConfig::PAGE_NUMBER_EXTEND_SIZE {
            for move_x in -CommonConfig::PAGE_NUMBER_EXTEND_SIZE..CommonConfig::PAGE_NUMBER_EXTEND_SIZE {
                let mut fill_rates = Vec::new();
                let mut tmp_coors = Vec::new();

                for coor in coors.iter() {
                    let new_coor = Coordinate {
                        x: coor.x + move_x,
                        y: coor.y + move_y,
                        w: coor.w,
                        h: coor.h,
                    };
                    let fill_rate = calculate_fill_rate(integral, &new_coor)?;
                    fill_rates.push(fill_rate);
                    tmp_coors.push(new_coor);
                }

                // 第一个坐标的填涂率必须大于阈值
                if fill_rates[0] < CommonConfig::PAGE_NUMBER_FILL_RATE {
                    continue;
                }

                let (_, variance) = otsu_threshold(&fill_rates);
                if variance > max_var {
                    refined_coors = tmp_coors;
                    max_var = variance;
                }
            }
        }

        Ok(refined_coors)
    }
}

/// 计算指定区域的填涂率（白色像素占比）
fn calculate_fill_rate(integral: &Vec<Vec<u64>>, coordinate: &Coordinate) -> Result<f64> {
    // 获取积分图尺寸
    let integral_rows = integral.len() as i32;
    let integral_cols = integral[0].len() as i32;

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
