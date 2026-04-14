use anyhow::{Context, Result};
use crate::config::{init_global_thread_pool, ImageProcessingConfig, VxPageConfig, VxConfig, FillPageConfig, AssistLocationPageConfig};
use crate::models::{MarkPaper, MobileOutput};
use crate::myutils::image::{calc_laplacian_variance, get_perspective_transform_matrix_with_boundary, get_perspective_transform_matrix_with_points, pers_trans_image, process_image};
use crate::myutils::myjson::from_json;
use crate::recognize::fill::RecFillModule;
use crate::recognize::location::LocationModule;
use crate::recognize::assist_location::AssistLocationModule;
use crate::recognize::page_number::PageNumberModule;
use crate::recognize::vx::RecVxModule;

/// 识别引擎
pub struct RecEngine {
    location_module: LocationModule,
    rec_fill_module: RecFillModule,
    assist_location_module: AssistLocationModule,
    rec_vx_module: RecVxModule,
    page_number_module: PageNumberModule,
    pub mark_paper: Option<MarkPaper>,
}

impl RecEngine {
    pub fn new_paper(mobile_input: &String) -> Result<Self> {
        let mut mark_paper: MarkPaper = from_json(mobile_input)?;
        init_global_thread_pool(mark_paper.num_threads);
        mark_paper.init_sort();
        Ok(Self {
            location_module: LocationModule::new(),
            assist_location_module: AssistLocationModule::new(),
            rec_fill_module: RecFillModule::new(),
            rec_vx_module: RecVxModule::new_paper(&mark_paper.vx_model_path)?,
            page_number_module: PageNumberModule::new(),
            mark_paper: Some(mark_paper),
        })
    }

    pub fn inference_paper(&self, image: &image::DynamicImage) -> Result<(MobileOutput, image::RgbImage)> {
        let mark = self.mark_paper.as_ref().context("引擎未初始化")?;
        let mark = &mark.resize(ImageProcessingConfig::PAPER_SCAN_TARGET_SCALE);
        let target_width = if mark.is_a4() {
            ImageProcessingConfig::TARGET_WIDTH_A4
        } else {
            ImageProcessingConfig::TARGET_WIDTH_A3
        };

        // 1. 处理图片
        let processed_image = process_image(image, target_width)?;

        // 输出 closed 图
        #[cfg(debug_assertions)]
        {
            let debug_path = "dev/test_data/debug/z_processed_closed.jpg";
            let _ = processed_image.closed_for_location.save(debug_path);
        }

        let mut baizheng = processed_image.clone();

        // 2. 定位检测
        let location = self.location_module.infer(&processed_image)?;

        #[cfg(debug_assertions)]
        {
            use crate::myutils::rendering::{render_quad, RenderMode, Colors};

            let mut render_image = processed_image.rgb.clone();
            let _ = render_quad(&mut render_image, &location, RenderMode::Hollow, Colors::green(), 2);
            let debug_path = "dev/test_data/debug/z_debug_location.jpg";
            let _ = render_image.save(debug_path);
        }

        // 3. 获取变换矩阵
        let tg_boundary = &mark.boundary;
        let pers_trans_matrix = get_perspective_transform_matrix_with_boundary(
            &location.to_points(),
            &tg_boundary.to_points(),
        )?;

        // 4. 第一次变换
        pers_trans_image(
            &mut baizheng,
            &pers_trans_matrix,
            tg_boundary.x + tg_boundary.w + ImageProcessingConfig::BOUNDARY_EXTEND_SIZE,
            tg_boundary.y + tg_boundary.h + ImageProcessingConfig::BOUNDARY_EXTEND_SIZE,
        )?;

        #[cfg(debug_assertions)]
        {
            let debug_path = "dev/test_data/debug/z_baizheng1_rgb.jpg";
            let _ = baizheng.rgb.save(debug_path);
        }

        // 5. 页码识别
        let page_index = self.page_number_module.infer(&baizheng, &mark.page_number)?;
        println!("识别到的页码索引: {}, 总页数: {}", page_index, mark.pages.len());

        let page_mark = mark.pages.get(page_index - 1)
            .context(format!("未找到对应的页码信息: page_index={}, pages.len()={}", page_index, mark.pages.len()))?;

        #[cfg(debug_assertions)]
        {
            let debug_path = "dev/test_data/debug/z_mor_for_assist.jpg";
            let _ = baizheng.closed.save(debug_path);
        }

        // 6. 找到辅助定位点
        let mut page_mark_assist_location = page_mark.assist_location.clone();
        let assist_location = self.assist_location_module.infer_paper::<AssistLocationPageConfig>(
            &baizheng,
            &mut page_mark_assist_location,
        )?;

        // 7. 获取变换矩阵
        let mut src = assist_location.to_points();
        let mut target = page_mark_assist_location.to_points();
        src.extend(mark.boundary.to_points());
        target.extend(mark.boundary.to_points());

        let pers_trans_matrix = get_perspective_transform_matrix_with_points(&src, &target)?;

        // 8. 第二次变换
        pers_trans_image(
            &mut baizheng,
            &pers_trans_matrix,
            mark.boundary.x + mark.boundary.w + ImageProcessingConfig::BOUNDARY_EXTEND_SIZE,
            mark.boundary.y + mark.boundary.h + ImageProcessingConfig::BOUNDARY_EXTEND_SIZE,
        )?;

        #[cfg(debug_assertions)]
        {
            let debug_path = "dev/test_data/debug/z_baizheng2_rgb.jpg";
            let _ = baizheng.rgb.save(debug_path);
        }

        // 9. 初始化输出
        let mut mobile_output = MobileOutput::new(&page_mark.rec_items);
        mobile_output.page_number = page_index - 1;

        // 10. 填涂识别
        self.rec_fill_module.infer::<FillPageConfig>(&baizheng, &mut mobile_output)?;

        // 10.5 vx区矫正
        self.rec_vx_module.refine_all_coordinates(&baizheng.closed, &mut mobile_output, 10, 2)?;

        // 11. vx框描黑
        let fix_image = if VxPageConfig::vx_model_channels() == 1 {
            &mut baizheng.gray
        } else {
            &mut baizheng.gray // 简化版本：都使用灰度图
        };
        self.rec_vx_module.render_vx_coordinate(fix_image, &mobile_output)?;

        // 12. vx识别
        if mark.num_threads > 1 {
            self.rec_vx_module.infer_parallel(&baizheng, &mut mobile_output)?;
        } else {
            self.rec_vx_module.infer(&baizheng, &mut mobile_output)?;
        }

        // 13. 计算lpls
        mobile_output.lpls = calc_laplacian_variance(&baizheng.gray)?;

        // 渲染最终结果
        #[cfg(debug_assertions)]
        {
            use crate::myutils::rendering::{render_output, render_assist_location, RenderMode, Colors};

            let mut render_image = image::DynamicImage::ImageLuma8(baizheng.gray.clone()).to_rgb8();
            let _ = render_output(&mut render_image, &mobile_output, RenderMode::Hollow, Colors::orange(), 2);
            let _ = render_assist_location(&mut render_image, &page_mark_assist_location, RenderMode::Hollow, Colors::red(), 1);

            let debug_path = "dev/test_data/debug/z_render_out.jpg";
            let _ = render_image.save(debug_path);

            println!("调试图片已保存到 dev/test_data/debug/");
        }

        Ok((mobile_output, baizheng.rgb))
    }
}
