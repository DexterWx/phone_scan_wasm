use anyhow::Result;
use image::{Rgb, RgbImage};
use imageproc::drawing::{draw_hollow_rect_mut, draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use ab_glyph::{FontRef, PxScale};
use crate::models::{AssistLocation, Coordinate, MobileOutput, Quad};

/// 渲染模式
#[derive(Debug, Clone, Copy)]
pub enum RenderMode {
    Filled,
    Hollow,
    Corners,
}

/// 预设颜色
pub struct Colors;
impl Colors {
    pub fn red() -> Rgb<u8> { Rgb([255, 0, 0]) }
    pub fn green() -> Rgb<u8> { Rgb([0, 255, 0]) }
    pub fn blue() -> Rgb<u8> { Rgb([0, 0, 255]) }
    pub fn yellow() -> Rgb<u8> { Rgb([255, 255, 0]) }
    pub fn orange() -> Rgb<u8> { Rgb([255, 165, 0]) }
    pub fn white() -> Rgb<u8> { Rgb([255, 255, 255]) }
    pub fn black() -> Rgb<u8> { Rgb([0, 0, 0]) }
}

/// 获取嵌入的字体
fn get_font() -> FontRef<'static> {
    let font_data = include_bytes!("../../assets/fonts/DejaVuSans.ttf");
    FontRef::try_from_slice(font_data).expect("Error loading font")
}

/// 渲染坐标
pub fn render_coordinate(
    image: &mut RgbImage,
    coord: &Coordinate,
    mode: RenderMode,
    color: Rgb<u8>,
    thickness: i32,
) -> Result<()> {
    let rect = Rect::at(coord.x, coord.y).of_size(coord.w as u32, coord.h as u32);

    match mode {
        RenderMode::Filled => {
            draw_filled_rect_mut(image, rect, color);
        }
        RenderMode::Hollow => {
            for t in 0..thickness {
                let expanded_rect = Rect::at(coord.x - t, coord.y - t)
                    .of_size((coord.w + t * 2) as u32, (coord.h + t * 2) as u32);
                draw_hollow_rect_mut(image, expanded_rect, color);
            }
        }
        RenderMode::Corners => {
            let radius = thickness * 2;
            let corners = [
                (coord.x, coord.y),
                (coord.x + coord.w, coord.y),
                (coord.x + coord.w, coord.y + coord.h),
                (coord.x, coord.y + coord.h),
            ];
            for (cx, cy) in &corners {
                imageproc::drawing::draw_filled_circle_mut(image, (*cx, *cy), radius, color);
            }
        }
    }
    Ok(())
}

/// 批量渲染坐标点
pub fn render_coordinates(
    image: &mut RgbImage,
    coords: &[Coordinate],
    mode: Option<RenderMode>,
    color: Option<Rgb<u8>>,
    thickness: Option<i32>,
) -> Result<()> {
    let mode = mode.unwrap_or(RenderMode::Hollow);
    let color = color.unwrap_or(Colors::red());
    let thickness = thickness.unwrap_or(2);

    for coord in coords {
        render_coordinate(image, coord, mode, color, thickness)?;
    }
    Ok(())
}

/// 渲染四边形
pub fn render_quad(
    image: &mut RgbImage,
    quad: &Quad,
    _mode: RenderMode,
    color: Rgb<u8>,
    thickness: i32,
) -> Result<()> {
    for i in 0..4 {
        let start = quad.points[i];
        let end = quad.points[(i + 1) % 4];
        for t in 0..thickness {
            imageproc::drawing::draw_line_segment_mut(
                image,
                (start.0 as f32, (start.1 + t) as f32),
                (end.0 as f32, (end.1 + t) as f32),
                color,
            );
        }
    }
    Ok(())
}

/// 渲染输出结果
pub fn render_output(
    image: &mut RgbImage,
    mobile_output: &MobileOutput,
    assist_location: &AssistLocation,
    mode: Option<RenderMode>,
    color: Option<Rgb<u8>>,
    thickness: Option<i32>,
    scale: Option<f64>,
) -> Result<()> {
    let mode = mode.unwrap_or(RenderMode::Hollow);
    let color = color.unwrap_or(Colors::red());
    let thickness = thickness.unwrap_or(2);
    let scale = scale.unwrap_or(1.0);

    // 如果需要缩放图像本身
    if scale != 1.0 {
        let new_width = (image.width() as f64 * scale) as u32;
        let new_height = (image.height() as f64 * scale) as u32;
        let resized = image::imageops::resize(
            image,
            new_width,
            new_height,
            image::imageops::FilterType::Triangle,
        );
        *image = resized;
    }

    // 获取字体
    let font = get_font();
    let font_scale = PxScale::from(12.0 * scale as f32);

    // 遍历所有识别结果
    for rec_result in &mobile_output.rec_results {
        // 遍历所有填涂项和对应的结果
        for (index, fill_item) in rec_result.rec_options.iter().enumerate() {
            // 根据缩放调整坐标
            let scaled_coord = Coordinate {
                x: (fill_item.coordinate.x as f64 * scale) as i32,
                y: (fill_item.coordinate.y as f64 * scale) as i32,
                w: (fill_item.coordinate.w as f64 * scale) as i32,
                h: (fill_item.coordinate.h as f64 * scale) as i32,
            };

            // 只有在rec_result为true时才绘制矩形框
            // 对于VX类型，根据class_id使用不同颜色渲染所有选项
            if rec_result.rec_type == crate::models::RecType::Vx {
                if rec_result.rec_result[index] {
                    render_coordinate(image, &scaled_coord, mode, color, thickness)?;
                }
            } else if rec_result.rec_type == crate::models::RecType::Location {
                render_coordinate(image, &scaled_coord, mode, color, thickness)?;
            } else if rec_result.rec_result[index] {
                // 渲染选中选项的坐标框
                render_coordinate(image, &scaled_coord, mode, color, thickness)?;
            }

            // 在选项上方渲染填涂率数字（保留两位小数）
            let text_x = (fill_item.coordinate.x as f64 * scale) as i32;
            let text_y = (fill_item.coordinate.y as f64 * scale - 5.0) as i32;

            // 格式化填涂率，保留两位小数
            let fill_rate_text = format!("{:.2}", fill_item.fill_rate);
            let text_color = if fill_item.class_id == 0 {
                Colors::yellow()
            } else {
                Colors::blue()
            };

            // 渲染文本
            draw_text_mut(
                image,
                text_color,
                text_x,
                text_y,
                font_scale,
                &font,
                &fill_rate_text,
            );
        }
    }

    // 渲染辅助定位点
    for assist_coord in assist_location.left.iter() {
        let scaled_coord = Coordinate {
            x: (assist_coord.x as f64 * scale) as i32,
            y: (assist_coord.y as f64 * scale) as i32,
            w: (assist_coord.w as f64 * scale) as i32,
            h: (assist_coord.h as f64 * scale) as i32,
        };
        render_coordinate(image, &scaled_coord, mode, color, 1)?;
    }
    for assist_coord in assist_location.right.iter() {
        let scaled_coord = Coordinate {
            x: (assist_coord.x as f64 * scale) as i32,
            y: (assist_coord.y as f64 * scale) as i32,
            w: (assist_coord.w as f64 * scale) as i32,
            h: (assist_coord.h as f64 * scale) as i32,
        };
        render_coordinate(image, &scaled_coord, mode, color, 1)?;
    }

    Ok(())
}

/// 渲染辅助定位点
pub fn render_assist_location(
    image: &mut RgbImage,
    assist_location: &AssistLocation,
    mode: RenderMode,
    color: Rgb<u8>,
    thickness: i32,
) -> Result<()> {
    for coord in &assist_location.left {
        render_coordinate(image, coord, mode, color, thickness)?;
    }
    for coord in &assist_location.right {
        render_coordinate(image, coord, mode, color, thickness)?;
    }
    Ok(())
}
