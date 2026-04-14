// 简化版渲染模块 - 不需要字体
use anyhow::Result;
use image::{Rgb, RgbImage};
use imageproc::drawing::{draw_hollow_rect_mut, draw_filled_rect_mut};
use imageproc::rect::Rect;
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
    mode: RenderMode,
    color: Rgb<u8>,
    thickness: i32,
) -> Result<()> {
    for rec_result in &mobile_output.rec_results {
        for (index, fill_item) in rec_result.rec_options.iter().enumerate() {
            if rec_result.rec_result[index] {
                render_coordinate(image, &fill_item.coordinate, mode, color, thickness)?;
            }
        }
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
