use anyhow::{Result, Context};
use image::{DynamicImage, GrayImage, GenericImageView, Luma, Rgb};
use imageproc::contrast::adaptive_threshold;
use imageproc::morphology;
use imageproc::geometric_transformations::{warp, Projection, Interpolation};
use nalgebra::DMatrix;
use crate::models::{ProcessedImage, Coordinate};
use crate::config::ImageProcessingConfig;

/// 读取图片
pub fn imread(path: &str) -> Result<DynamicImage> {
    image::open(path).context("读取图片失败")
}

/// 处理图片：缩放、灰度化、二值化、形态学处理
pub fn process_image(image: &DynamicImage, target_width: u32) -> Result<ProcessedImage> {
    // 1. 缩放图片 - 使用 Triangle 插值替代 Lanczos3，速度提升 2-3 倍
    let (width, height) = image.dimensions();
    let scale = target_width as f32 / width as f32;
    let new_height = (height as f32 * scale) as u32;
    let resized = image.resize_exact(target_width, new_height, image::imageops::FilterType::Triangle);

    // 2. 转换为RGB和灰度图
    let rgb = resized.to_rgb8();
    let gray = resized.to_luma8();

    // 3. 高斯模糊
    let blurred = imageproc::filter::gaussian_blur_f32(&gray, ImageProcessingConfig::GAUSSIAN_SIGMA);

    // 4. 自适应阈值（使用 BinaryInverted 使背景为黑色，线条为白色）
    let thresh = threshold_adaptive(&blurred);
    // let thresh = threshold_otsu(&blurred);
    // debug 二值图
    #[cfg(debug_assertions)]
    {
        let debug_path = "dev/test_data/debug/z_processed_thresh.jpg";
        let _ = thresh.save(debug_path);
    }

    // 5. 形态学闭运算
    let closed = morphology_close(&thresh, ImageProcessingConfig::MORPH_KERNEL);

    // 6. 针对定位的形态学处理
    let opened_for_location = morphology_open(&thresh, ImageProcessingConfig::MORPH_KERNEL_OPEN_FOR_LOCATION);
    let closed_for_location = morphology_close(&opened_for_location, ImageProcessingConfig::MORPH_KERNEL_CLOSE_FOR_LOCATION);

    Ok(ProcessedImage {
        rgb,
        gray,
        thresh,
        closed,
        closed_for_location,
    })
}

/// Otsu二值化（反转：背景黑色，前景白色）
fn threshold_otsu(image: &GrayImage) -> GrayImage {
    let threshold = imageproc::contrast::otsu_level(image);
    imageproc::contrast::threshold(image, threshold, imageproc::contrast::ThresholdType::BinaryInverted)
}

/// 自适应阈值（反转：背景黑色，前景白色）
fn threshold_adaptive(image: &GrayImage) -> GrayImage {
    let block_radius = 9; // 可调（窗口大小 = 2r+1）
    let delta = 5;        // 可调（阈值偏移）

    let mut thresh = adaptive_threshold(image, block_radius, delta);
    image::imageops::invert(&mut thresh);
    thresh
}

/// 形态学闭运算
fn morphology_close(image: &GrayImage, kernel_size: u32) -> GrayImage {
    let dilated = morphology::dilate(image, imageproc::distance_transform::Norm::LInf, kernel_size as u8);
    morphology::erode(&dilated, imageproc::distance_transform::Norm::LInf, kernel_size as u8)
}

/// 形态学开运算
fn morphology_open(image: &GrayImage, kernel_size: u32) -> GrayImage {
    let eroded = morphology::erode(image, imageproc::distance_transform::Norm::LInf, kernel_size as u8);
    morphology::dilate(&eroded, imageproc::distance_transform::Norm::LInf, kernel_size as u8)
}

/// 计算拉普拉斯方差（清晰度）
pub fn calc_laplacian_variance(image: &GrayImage) -> Result<f64> {
    // 简化版本：计算图像梯度的方差
    let mut sum = 0.0;
    let mut sum_sq = 0.0;
    let mut count = 0;

    for y in 1..image.height()-1 {
        for x in 1..image.width()-1 {
            let center = image.get_pixel(x, y)[0] as f64;
            let left = image.get_pixel(x-1, y)[0] as f64;
            let right = image.get_pixel(x+1, y)[0] as f64;
            let top = image.get_pixel(x, y-1)[0] as f64;
            let bottom = image.get_pixel(x, y+1)[0] as f64;

            let laplacian = (4.0 * center - left - right - top - bottom).abs();
            sum += laplacian;
            sum_sq += laplacian * laplacian;
            count += 1;
        }
    }

    let mean = sum / count as f64;
    let variance = (sum_sq / count as f64) - (mean * mean);
    Ok(variance)
}

/// 计算透视变换 - 使用 imageproc 的 Projection::from_control_points
/// 使用 SVD 求解透视变换矩阵（支持任意数量的点，至少4个）
fn solve_homography_svd(src: &[(f32, f32)], dst: &[(f32, f32)]) -> Result<[f32; 9]> {
    let n = src.len();
    if n < 4 {
        anyhow::bail!("透视变换至少需要4个点");
    }

    // 构建 2n x 8 的矩阵 A 和 2n x 1 的向量 b
    // 参考 imageproc 的实现方式
    let mut a_data = Vec::with_capacity(2 * n * 8);
    let mut b_data = Vec::with_capacity(2 * n);

    for i in 0..n {
        let (xf, yf) = (src[i].0 as f64, src[i].1 as f64);
        let (x, y) = (dst[i].0 as f64, dst[i].1 as f64);

        // 第一行: [0, 0, 0, -xf, -yf, -1, y*xf, y*yf]  =>  b: -y
        a_data.extend_from_slice(&[0.0, 0.0, 0.0, -xf, -yf, -1.0, y * xf, y * yf]);
        b_data.push(-y);

        // 第二行: [xf, yf, 1, 0, 0, 0, -x*xf, -x*yf]  =>  b: x
        a_data.extend_from_slice(&[xf, yf, 1.0, 0.0, 0.0, 0.0, -x * xf, -x * yf]);
        b_data.push(x);
    }

    let a = DMatrix::from_row_slice(2 * n, 8, &a_data);
    let b = DMatrix::from_row_slice(2 * n, 1, &b_data);

    // 使用 SVD 求解 Ah = b
    let svd = a.svd(true, true);
    let h = svd.solve(&b, 1e-10)
        .map_err(|_| anyhow::anyhow!("SVD 求解失败"))?;

    // 构建 3x3 矩阵 [h0, h1, h2, h3, h4, h5, h6, h7, 1.0]
    let mut result = [0.0f32; 9];
    for i in 0..8 {
        result[i] = h[i] as f32;
    }
    result[8] = 1.0;

    Ok(result)
}

pub fn get_perspective_transform_with_boundary(
    src: &Vec<(f32, f32)>,
    dst: &Vec<(f32, f32)>,
) -> Result<Projection> {
    if src.len() != 4 || dst.len() != 4 {
        anyhow::bail!("透视变换需要4个点");
    }

    let matrix = solve_homography_svd(src, dst)?;
    Projection::from_matrix(matrix)
        .ok_or_else(|| anyhow::anyhow!("无法计算透视变换矩阵"))
}

pub fn get_perspective_transform_with_points(
    src: &Vec<(f32, f32)>,
    dst: &Vec<(f32, f32)>,
) -> Result<Projection> {
    let n = src.len().min(dst.len());

    if n < 4 {
        anyhow::bail!("透视变换至少需要4个点");
    }

    // 使用所有点求解（最小二乘）
    let matrix = solve_homography_svd(src, dst)?;
    Projection::from_matrix(matrix)
        .ok_or_else(|| anyhow::anyhow!("无法计算透视变换矩阵"))
}

/// 应用透视变换 - 直接使用 Projection 对象
pub fn pers_trans_image(
    image: &mut ProcessedImage,
    projection: &Projection,
    width: i32,
    height: i32,
) -> Result<()> {
    let w = width.max(0) as u32;
    let h = height.max(0) as u32;

    // 对每个图像应用透视变换
    image.rgb = warp(&image.rgb, projection, Interpolation::Bilinear, Rgb([255u8, 255u8, 255u8]));
    image.gray = warp(&image.gray, projection, Interpolation::Bilinear, Luma([255u8]));
    image.thresh = warp(&image.thresh, projection, Interpolation::Nearest, Luma([255u8]));
    image.closed = warp(&image.closed, projection, Interpolation::Nearest, Luma([255u8]));
    image.closed_for_location = warp(&image.closed_for_location, projection, Interpolation::Nearest, Luma([255u8]));

    // 裁剪到目标尺寸
    image.rgb = image::imageops::crop(&mut image.rgb, 0, 0, w, h).to_image();
    image.gray = image::imageops::crop(&mut image.gray, 0, 0, w, h).to_image();
    image.thresh = image::imageops::crop(&mut image.thresh, 0, 0, w, h).to_image();
    image.closed = image::imageops::crop(&mut image.closed, 0, 0, w, h).to_image();
    image.closed_for_location = image::imageops::crop(&mut image.closed_for_location, 0, 0, w, h).to_image();

    Ok(())
}

/// 计算积分图
pub fn integral_image(image: &GrayImage) -> Result<Vec<Vec<u64>>> {
    let (width, height) = image.dimensions();
    let mut integral = vec![vec![0u64; width as usize + 1]; height as usize + 1];

    for y in 0..height {
        for x in 0..width {
            let pixel_value = image.get_pixel(x, y)[0] as u64;
            integral[y as usize + 1][x as usize + 1] =
                pixel_value +
                integral[y as usize][x as usize + 1] +
                integral[y as usize + 1][x as usize] -
                integral[y as usize][x as usize];
        }
    }

    Ok(integral)
}

/// 使用积分图计算区域和
pub fn sum_pixel(integral: &Vec<Vec<u64>>, coord: &Coordinate) -> u64 {
    let x1 = coord.x.max(0) as usize;
    let y1 = coord.y.max(0) as usize;
    let x2 = (coord.x + coord.w).max(0) as usize;
    let y2 = (coord.y + coord.h).max(0) as usize;

    if y2 >= integral.len() || x2 >= integral[0].len() {
        return 0;
    }

    integral[y2][x2] + integral[y1][x1] - integral[y1][x2] - integral[y2][x1]
}

/// 合并多个坐标为一个大的坐标区域
pub fn merge_coordinates(coordinates: &Vec<Coordinate>, extend_size_w: i32, extend_size_h: i32) -> Coordinate {
    let mut x = coordinates.iter().map(|c| c.x).min().unwrap();
    let mut y = coordinates.iter().map(|c| c.y).min().unwrap();
    let mut w = coordinates.iter().map(|c| c.x + c.w).max().unwrap() - x;
    let mut h = coordinates.iter().map(|c| c.y + c.h).max().unwrap() - y;

    x -= extend_size_w;
    y -= extend_size_h;
    w += extend_size_w * 2;
    h += extend_size_h * 2;

    Coordinate { x, y, w, h }
}
