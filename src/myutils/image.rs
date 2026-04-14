use anyhow::{Result, Context};
use image::{DynamicImage, GrayImage, GenericImageView, Luma, Rgb};
use imageproc::morphology;
use imageproc::geometric_transformations::{warp, Projection, Interpolation};
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

    // 4. 自适应阈值（简化版本：使用全局阈值）
    let thresh = threshold_otsu(&blurred);

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

/// Otsu二值化
fn threshold_otsu(image: &GrayImage) -> GrayImage {
    let threshold = imageproc::contrast::otsu_level(image);
    imageproc::contrast::threshold(image, threshold, imageproc::contrast::ThresholdType::Binary)
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

/// 透视变换
pub fn get_perspective_transform_matrix_with_boundary(
    src: &Vec<(f32, f32)>,
    dst: &Vec<(f32, f32)>,
) -> Result<[[f64; 3]; 3]> {
    if src.len() != 4 || dst.len() != 4 {
        anyhow::bail!("透视变换需要4个点");
    }

    // 计算透视变换矩阵
    // 使用 DLT (Direct Linear Transform) 算法
    let mut a = vec![vec![0.0; 8]; 8];
    let mut b = vec![0.0; 8];

    for i in 0..4 {
        let (x, y) = src[i];
        let (u, v) = dst[i];

        a[i * 2] = vec![x as f64, y as f64, 1.0, 0.0, 0.0, 0.0, -u as f64 * x as f64, -u as f64 * y as f64];
        a[i * 2 + 1] = vec![0.0, 0.0, 0.0, x as f64, y as f64, 1.0, -v as f64 * x as f64, -v as f64 * y as f64];

        b[i * 2] = u as f64;
        b[i * 2 + 1] = v as f64;
    }

    // 使用高斯消元法求解线性方程组
    let h = solve_linear_system(&a, &b)?;

    Ok([
        [h[0], h[1], h[2]],
        [h[3], h[4], h[5]],
        [h[6], h[7], 1.0],
    ])
}

pub fn get_perspective_transform_matrix_with_points(
    src: &Vec<(f32, f32)>,
    dst: &Vec<(f32, f32)>,
) -> Result<[[f64; 3]; 3]> {
    // 如果点数大于4，只使用前4个点
    let src_4: Vec<(f32, f32)> = src.iter().take(4).copied().collect();
    let dst_4: Vec<(f32, f32)> = dst.iter().take(4).copied().collect();

    if src_4.len() < 4 || dst_4.len() < 4 {
        anyhow::bail!("透视变换至少需要4个点");
    }

    get_perspective_transform_matrix_with_boundary(&src_4, &dst_4)
}

/// 高斯消元法求解线性方程组
fn solve_linear_system(a: &Vec<Vec<f64>>, b: &Vec<f64>) -> Result<Vec<f64>> {
    let n = a.len();
    let mut aug = vec![vec![0.0; n + 1]; n];

    // 构建增广矩阵
    for i in 0..n {
        for j in 0..n {
            aug[i][j] = a[i][j];
        }
        aug[i][n] = b[i];
    }

    // 高斯消元
    for i in 0..n {
        // 找到主元
        let mut max_row = i;
        for k in i + 1..n {
            if aug[k][i].abs() > aug[max_row][i].abs() {
                max_row = k;
            }
        }

        // 交换行
        aug.swap(i, max_row);

        // 消元
        for k in i + 1..n {
            let factor = aug[k][i] / aug[i][i];
            for j in i..=n {
                aug[k][j] -= factor * aug[i][j];
            }
        }
    }

    // 回代
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        x[i] = aug[i][n];
        for j in i + 1..n {
            x[i] -= aug[i][j] * x[j];
        }
        x[i] /= aug[i][i];
    }

    Ok(x)
}

/// 应用透视变换 - 使用 imageproc 的优化实现
pub fn pers_trans_image(
    image: &mut ProcessedImage,
    matrix: &[[f64; 3]; 3],
    width: i32,
    height: i32,
) -> Result<()> {
    let w = width.max(0) as u32;
    let h = height.max(0) as u32;

    // 将 f64 矩阵转换为 f32（imageproc 使用 f32）
    let matrix_f32 = [
        matrix[0][0] as f32, matrix[0][1] as f32, matrix[0][2] as f32,
        matrix[1][0] as f32, matrix[1][1] as f32, matrix[1][2] as f32,
        matrix[2][0] as f32, matrix[2][1] as f32, matrix[2][2] as f32,
    ];

    // 创建 Projection
    let projection = Projection::from_matrix(matrix_f32)
        .ok_or_else(|| anyhow::anyhow!("无法创建透视变换"))?;

    // 对每个图像应用透视变换 - 使用 imageproc 的优化实现
    image.rgb = warp(&image.rgb, &projection, Interpolation::Bilinear, Rgb([255u8, 255u8, 255u8]));
    image.gray = warp(&image.gray, &projection, Interpolation::Bilinear, Luma([255u8]));
    image.thresh = warp(&image.thresh, &projection, Interpolation::Nearest, Luma([255u8]));
    image.closed = warp(&image.closed, &projection, Interpolation::Nearest, Luma([255u8]));
    image.closed_for_location = warp(&image.closed_for_location, &projection, Interpolation::Nearest, Luma([255u8]));

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
