/// 图像处理配置参数
pub struct ImageProcessingConfig;

impl ImageProcessingConfig {
    /// 高斯模糊核大小
    pub const GAUSSIAN_KERNEL_SIZE: u32 = 5;

    /// 高斯模糊sigma值
    pub const GAUSSIAN_SIGMA: f32 = 1.0;

    /// 统一输入图像的宽度
    pub const TARGET_WIDTH_A4: u32 = 2400;
    pub const TARGET_WIDTH_A3: u32 = 4000;

    /// 目标图片缩放比例
    pub const PAPER_SCAN_TARGET_SCALE: f64 = 2.0;

    /// 自适应阈值的块大小
    pub const BLOCK_SIZE: u32 = 51;

    /// 自适应阈值的常数
    pub const C: i32 = 5;

    /// 形态学操作的核大小
    pub const MORPH_KERNEL: u32 = 3;
    pub const MORPH_KERNEL_OPEN_FOR_LOCATION: u32 = 3;
    pub const MORPH_KERNEL_CLOSE_FOR_LOCATION: u32 = 5;

    /// 多边形逼近的epsilon因子
    pub const EPSILON_FACTOR: f64 = 0.015;

    /// 最小面积占比
    pub const MIN_AREA_RATIO: f64 = 0.25;

    /// 边界惩罚系数
    pub const MARGIN_PENALTY: f64 = 50.0;

    /// 变换后，从边界向外拓展的距离
    pub const BOUNDARY_EXTEND_SIZE: i32 = 40;

    /// 边界贴合距离
    pub const BOUNDARY_PENALTY: f64 = 12.0;
}

/// 辅助定位点的寻找
pub trait AssistLocationConfig {
    fn assist_area_extend_size_h() -> i32;
    fn assist_area_extend_size_w() -> i32;
    fn assist_point_min_size() -> i32;
    fn assist_point_max_size() -> i32;
    fn assist_point_min_area() -> f64;
    fn assist_point_max_area() -> f64;
    fn assist_point_min_fill_ratio() -> f64;
    fn assist_point_whdiff_max() -> i32;
    fn assist_point_x_median_diff() -> i32;
}

pub struct AssistLocationPageConfig;
impl AssistLocationConfig for AssistLocationPageConfig {
    fn assist_area_extend_size_h() -> i32 { 35 }
    fn assist_area_extend_size_w() -> i32 { 20 }
    fn assist_point_min_size() -> i32 { 8 }
    fn assist_point_max_size() -> i32 { 15 }
    fn assist_point_min_area() -> f64 { 80.0 }
    fn assist_point_max_area() -> f64 { 170.0 }
    fn assist_point_min_fill_ratio() -> f64 { 0.88 }
    fn assist_point_whdiff_max() -> i32 { 4 }
    fn assist_point_x_median_diff() -> i32 { 18 }
}

pub trait FillConfig {
    fn fill_rate_min() -> f64;
    fn refine_coor_range() -> i32;
    fn gray_contrast_enhance() -> f32;
}

pub struct FillPageConfig;
impl FillConfig for FillPageConfig {
    fn fill_rate_min() -> f64 { 0.4 }
    fn refine_coor_range() -> i32 { 4 }
    fn gray_contrast_enhance() -> f32 { 10.0 }

}

pub struct CommonConfig;
impl CommonConfig {
    pub const PAGE_NUMBER_FILL_RATE: f64 = 0.6;
    pub const PAGE_NUMBER_EXTEND_SIZE: i32 = 20;
}

/// VX单线识别配置
pub trait VxConfig {
    fn fill_ratio_min() -> f64;
    fn fill_ratio_max() -> f64;
    fn vx_model_channels() -> i32;
    fn vx_model_height() -> i32;
    fn vx_model_width() -> i32;
    fn vx_model_padding_value() -> i32;
    fn vx_box_expand_size() -> i32;
}

pub struct VxPageConfig;
impl VxConfig for VxPageConfig {
    fn fill_ratio_min() -> f64 { 0.03 }
    fn fill_ratio_max() -> f64 { 0.5 }
    fn vx_model_channels() -> i32 { 1 }
    fn vx_model_height() -> i32 { 36 }
    fn vx_model_width() -> i32 { 50 }
    fn vx_model_padding_value() -> i32 { 0 }
    fn vx_box_expand_size() -> i32 { 4 }
}
