use anyhow::Result;
use crate::models::{ProcessedImage, Coordinate};
use crate::config::CommonConfig;
use crate::myutils::image::{integral_image, sum_pixel};

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

        // 找到所有填涂的页码点
        let mut filled_indices = Vec::new();

        for (idx, coord) in page_numbers.iter().enumerate() {
            // 扩展区域
            let extended_coord = Coordinate {
                x: coord.x - CommonConfig::PAGE_NUMBER_EXTEND_SIZE,
                y: coord.y - CommonConfig::PAGE_NUMBER_EXTEND_SIZE,
                w: coord.w + CommonConfig::PAGE_NUMBER_EXTEND_SIZE * 2,
                h: coord.h + CommonConfig::PAGE_NUMBER_EXTEND_SIZE * 2,
            };

            let area = (extended_coord.w * extended_coord.h) as u64;
            if area == 0 {
                continue;
            }

            let black_pixels = sum_pixel(&integral, &extended_coord);
            let fill_rate = black_pixels as f64 / (area as f64 * 255.0);

            if fill_rate > CommonConfig::PAGE_NUMBER_FILL_RATE {
                filled_indices.push(idx);
            }
        }

        // 根据填涂的页码点计算页码
        // 通常页码编码方式：第1位表示1，第2位表示2，第3位表示4，第4位表示8...
        // 或者简单地：填涂的点的位置就是页码

        // 这里使用简单的方式：如果有填涂的点，取第一个填涂点的索引+1作为页码
        if !filled_indices.is_empty() {
            // 如果有多个填涂点，可能是二进制编码
            // 简化处理：取最小的索引+1
            Ok(filled_indices[0] + 1)
        } else {
            // 如果没有填涂的点，返回第一页
            Ok(1)
        }
    }
}
