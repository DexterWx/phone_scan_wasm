use anyhow::Result;
use crate::models::{ProcessedImage, MobileOutput, RecType};
use crate::config::FillConfig;
use crate::myutils::image::{integral_image, sum_pixel};

pub struct RecFillModule;

impl RecFillModule {
    pub fn new() -> Self {
        Self
    }

    pub fn infer<T: FillConfig>(&self, process_image: &ProcessedImage, mobile_output: &mut MobileOutput) -> Result<()> {
        // 计算积分图
        let integral = integral_image(&process_image.thresh)?;

        // 计算所有选项的填涂率
        self.calculate_all_fill_rate(&integral, mobile_output)?;

        // 计算填涂率阈值
        let fill_rates: Vec<f64> = mobile_output.rec_results.iter()
            .filter(|rec_result| [RecType::SingleChoice, RecType::MultipleChoice].contains(&rec_result.rec_type))
            .flat_map(|rec_result| rec_result.rec_options.iter().map(|item| item.fill_rate))
            .collect();

        let (mut thresh, _) = crate::myutils::math::otsu_threshold(&fill_rates);
        thresh = thresh.max(T::fill_rate_min());
        thresh = (thresh * 100.0).ceil() / 100.0;

        // 设置填涂结果
        self.set_default_fill(mobile_output, thresh)?;

        Ok(())
    }

    fn calculate_all_fill_rate(&self, integral: &Vec<Vec<u64>>, mobile_output: &mut MobileOutput) -> Result<()> {
        for rec_result in mobile_output.rec_results.iter_mut() {
            if ![RecType::SingleChoice, RecType::MultipleChoice].contains(&rec_result.rec_type) {
                continue;
            }

            for rec_option in rec_result.rec_options.iter_mut() {
                let coord = &rec_option.coordinate;
                let area = (coord.w * coord.h) as u64;
                if area == 0 {
                    rec_option.fill_rate = 0.0;
                    continue;
                }

                let black_pixels = sum_pixel(integral, coord);
                // 注意：二值图中黑色为0，白色为255
                rec_option.fill_rate = black_pixels as f64 / (area as f64 * 255.0);
            }
        }
        Ok(())
    }

    fn set_default_fill(&self, mobile_output: &mut MobileOutput, thresh: f64) -> Result<()> {
        for rec_result in mobile_output.rec_results.iter_mut() {
            if ![RecType::SingleChoice, RecType::MultipleChoice].contains(&rec_result.rec_type) {
                continue;
            }

            for (index, fill_item) in rec_result.rec_options.iter_mut().enumerate() {
                if fill_item.fill_rate > thresh {
                    rec_result.rec_result[index] = true;
                    fill_item.class_id = 0;
                } else {
                    rec_result.rec_result[index] = false;
                    fill_item.class_id = 1;
                }
            }
        }
        Ok(())
    }
}
