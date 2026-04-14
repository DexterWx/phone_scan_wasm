use anyhow::Result;
use crate::models::{ProcessedImage, MobileOutput};

pub struct RecVxModule;

impl RecVxModule {
    pub fn new_paper(_model_path: &str) -> Result<Self> {
        // 简化版本：不加载模型
        Ok(Self)
    }

    pub fn refine_all_coordinates(
        &self,
        _closed: &image::GrayImage,
        _mobile_output: &mut MobileOutput,
        _expand_size: i32,
        _shrink_size: i32,
    ) -> Result<()> {
        // 简化版本：跳过坐标矫正
        Ok(())
    }

    pub fn render_vx_coordinate(
        &self,
        _image: &mut image::GrayImage,
        _mobile_output: &MobileOutput,
    ) -> Result<()> {
        // 简化版本：跳过渲染
        Ok(())
    }

    pub fn infer(&self, _processed_image: &ProcessedImage, _mobile_output: &mut MobileOutput) -> Result<()> {
        // 简化版本：跳过VX识别
        Ok(())
    }

    pub fn infer_parallel(&self, processed_image: &ProcessedImage, mobile_output: &mut MobileOutput) -> Result<()> {
        // 简化版本：调用单线程版本
        self.infer(processed_image, mobile_output)
    }
}
