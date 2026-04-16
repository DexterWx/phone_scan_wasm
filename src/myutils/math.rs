use anyhow::Result;
use crate::models::{Quad, ContourInfo};

/// 实现Otsu阈值算法，用于计算一维直方图的最佳分割阈值
pub fn otsu_threshold(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }

    const NUM_BINS: usize = 1000;
    let mut histogram = [0usize; NUM_BINS];

    for &value in values {
        let clamped_value = value.max(0.0).min(1.0);
        let bin_index = (clamped_value * (NUM_BINS - 1) as f64) as usize;
        let bin_index = bin_index.min(NUM_BINS - 1);
        histogram[bin_index] += 1;
    }

    let mut cumulative_histogram = [0usize; NUM_BINS];
    let mut cumulative_moments = [0.0f64; NUM_BINS];

    cumulative_histogram[0] = histogram[0];
    cumulative_moments[0] = 0.0 * histogram[0] as f64;

    for i in 1..NUM_BINS {
        cumulative_histogram[i] = cumulative_histogram[i - 1] + histogram[i];
        cumulative_moments[i] = cumulative_moments[i - 1] + (i as f64) * histogram[i] as f64;
    }

    let total_pixels = cumulative_histogram[NUM_BINS - 1];
    let total_moments = cumulative_moments[NUM_BINS - 1];

    if total_pixels == 0 {
        return (0.0, 0.0);
    }

    let mut max_variance = 0.0;
    let mut best_threshold = 0.0;

    for i in 0..NUM_BINS - 1 {
        let pixels_background = cumulative_histogram[i];
        let pixels_foreground = total_pixels - pixels_background;

        if pixels_background == 0 || pixels_foreground == 0 {
            continue;
        }

        let moment_background = cumulative_moments[i];
        let moment_foreground = total_moments - moment_background;

        let mean_background = moment_background / pixels_background as f64;
        let mean_foreground = moment_foreground / pixels_foreground as f64;

        let diff = mean_background - mean_foreground;
        let variance = (pixels_background as f64) * (pixels_foreground as f64) * diff * diff;

        if variance > max_variance {
            max_variance = variance;
            best_threshold = (i as f64) / (NUM_BINS - 1) as f64;
        }
    }

    (best_threshold, max_variance)
}

/// 计算两点之间的欧氏距离
pub fn distance(point1: &(f32, f32), point2: &(f32, f32)) -> f32 {
    let dx = point1.0 - point2.0;
    let dy = point1.1 - point2.1;
    (dx * dx + dy * dy).sqrt()
}

/// 给第一个点集的每个点匹配一个距离最近的点
pub fn match_points(points1: &Vec<(f32, f32)>, points2: &Vec<(f32, f32)>) -> Result<Vec<(f32, f32)>> {
    if points1.len() > points2.len() {
        anyhow::bail!("划分矫正数量不匹配");
    }
    let mut res = Vec::new();
    for point1 in points1 {
        let mut min_distance = f32::MAX;
        let mut min_index = 0;
        for (index, point2) in points2.iter().enumerate() {
            let dist = distance(point1, point2);
            if dist < min_distance {
                min_distance = dist;
                min_index = index;
            }
        }
        res.push(points2[min_index]);
    }
    Ok(res)
}

/// 计算两个 i32 点之间的欧氏距离
pub fn distance_point2i(a: &(i32, i32), b: &(i32, i32)) -> f64 {
    let dx = (a.0 - b.0) as f64;
    let dy = (a.1 - b.1) as f64;
    (dx * dx + dy * dy).sqrt()
}

/// 计算点到线段的距离
fn point_to_segment_distance(point: (i32, i32), line_start: (i32, i32), line_end: (i32, i32)) -> f64 {
    let px = point.0 as f64;
    let py = point.1 as f64;
    let x1 = line_start.0 as f64;
    let y1 = line_start.1 as f64;
    let x2 = line_end.0 as f64;
    let y2 = line_end.1 as f64;

    let line_length_sq = (x2 - x1) * (x2 - x1) + (y2 - y1) * (y2 - y1);

    if line_length_sq == 0.0 {
        return ((px - x1) * (px - x1) + (py - y1) * (py - y1)).sqrt();
    }

    let t = (((px - x1) * (x2 - x1) + (py - y1) * (y2 - y1)) / line_length_sq)
        .max(0.0)
        .min(1.0);

    let proj_x = x1 + t * (x2 - x1);
    let proj_y = y1 + t * (y2 - y1);

    ((px - proj_x) * (px - proj_x) + (py - proj_y) * (py - proj_y)).sqrt()
}

/// 计算单个点到 Quad 四条边的最近距离
pub fn point_to_quad_min_distance(point: (i32, i32), boundary: &Quad) -> f64 {
    let edges = [
        (boundary.points[0], boundary.points[1]),
        (boundary.points[1], boundary.points[2]),
        (boundary.points[2], boundary.points[3]),
        (boundary.points[3], boundary.points[0]),
    ];
    edges
        .iter()
        .map(|&(p1, p2)| point_to_segment_distance(point, p1, p2))
        .fold(f64::MAX, f64::min)
}

/// 计算 ContourInfo 上所有点到 Quad 四条边的平均最近距离
pub fn avg_distance_contour_to_quad(boundary: &Quad, contour: &ContourInfo) -> f64 {
    if contour.points.is_empty() {
        return f64::MAX;
    }

    let edges = [
        (boundary.points[0], boundary.points[1]),
        (boundary.points[1], boundary.points[2]),
        (boundary.points[2], boundary.points[3]),
        (boundary.points[3], boundary.points[0]),
    ];

    let mut total_distance = 0.0;
    let mut count = 0;

    for &point in &contour.points {
        let min_distance = edges
            .iter()
            .map(|&(p1, p2)| point_to_segment_distance(point, p1, p2))
            .fold(f64::MAX, f64::min);

        total_distance += min_distance;
        count += 1;
    }

    if count == 0 {
        return f64::MAX;
    }

    total_distance / count as f64
}
