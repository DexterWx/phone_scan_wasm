use serde::{Deserialize, Serialize};

/// 坐标信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coordinate {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Coordinate {
    pub fn resize(&self, scale: f64) -> Coordinate {
        Coordinate {
            x: (self.x as f64 * scale) as i32,
            y: (self.y as f64 * scale) as i32,
            w: (self.w as f64 * scale) as i32,
            h: (self.h as f64 * scale) as i32,
        }
    }

    pub fn to_points(&self) -> Vec<(f32, f32)> {
        vec![
            (self.x as f32, self.y as f32),
            ((self.x + self.w) as f32, self.y as f32),
            ((self.x + self.w) as f32, (self.y + self.h) as f32),
            (self.x as f32, (self.y + self.h) as f32),
        ]
    }

    pub fn get_center(&self) -> (f32, f32) {
        (
            (self.x + self.w / 2) as f32,
            (self.y + self.h / 2) as f32,
        )
    }
}

/// 非矩形四边形
#[derive(Debug, Clone)]
pub struct Quad {
    pub points: [(i32, i32); 4],
}

impl Quad {
    pub fn to_points(&self) -> Vec<(f32, f32)> {
        self.points
            .iter()
            .map(|(x, y)| (*x as f32, *y as f32))
            .collect()
    }

    pub fn to_coordinate(&self) -> Coordinate {
        Coordinate {
            x: self.points[0].0,
            y: self.points[0].1,
            w: self.points[1].0 - self.points[0].0,
            h: self.points[2].1 - self.points[0].1,
        }
    }
}

/// 处理后的图片数据
#[derive(Debug, Clone)]
pub struct ProcessedImage {
    pub rgb: image::RgbImage,
    pub gray: image::GrayImage,
    pub thresh: image::GrayImage,
    pub closed: image::GrayImage,
    pub closed_for_location: image::GrayImage,
}

/// 识别类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "i32", into = "i32")]
pub enum RecType {
    SingleChoice = 1,
    MultipleChoice = 2,
    Vx = 3,
    HandWriting = 4,
    Location = 5,
}

impl From<i32> for RecType {
    fn from(value: i32) -> Self {
        match value {
            1 => RecType::SingleChoice,
            2 => RecType::MultipleChoice,
            3 => RecType::Vx,
            4 => RecType::HandWriting,
            5 => RecType::Location,
            _ => RecType::SingleChoice,
        }
    }
}

impl From<RecType> for i32 {
    fn from(rec_type: RecType) -> Self {
        rec_type as i32
    }
}

/// 识别项目信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecItem {
    pub rec_type: RecType,
    pub sub_options: Vec<Coordinate>,
}

impl RecItem {
    pub fn resize(&self, scale: f64) -> RecItem {
        RecItem {
            rec_type: self.rec_type,
            sub_options: self.sub_options.iter().map(|coor| coor.resize(scale)).collect(),
        }
    }
}

/// 辅助定位点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistLocation {
    pub left: Vec<Coordinate>,
    pub right: Vec<Coordinate>,
}

impl AssistLocation {
    pub fn resize(&self, scale: f64) -> AssistLocation {
        AssistLocation {
            left: self.left.iter().map(|coor| coor.resize(scale)).collect(),
            right: self.right.iter().map(|coor| coor.resize(scale)).collect(),
        }
    }

    pub fn init_sort(&mut self) {
        self.left.sort_by(|a, b| match a.x.cmp(&b.x) {
            std::cmp::Ordering::Equal => a.y.cmp(&b.y),
            other => other,
        });
        self.right.sort_by(|a, b| match a.x.cmp(&b.x) {
            std::cmp::Ordering::Equal => a.y.cmp(&b.y),
            other => other,
        });
    }

    pub fn to_points(&self) -> Vec<(f32, f32)> {
        let mut points = Vec::new();
        for coord in &self.left {
            points.push(coord.get_center());
        }
        for coord in &self.right {
            points.push(coord.get_center());
        }
        points
    }
}

/// 识别结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecResult {
    pub rec_result: Vec<bool>,
    pub rec_options: Vec<RecOption>,
    pub rec_type: RecType,
}

/// 识别选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecOption {
    pub fill_rate: f64,
    pub coordinate: Coordinate,
    pub class_id: u8,
}

/// 输出数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileOutput {
    pub code: i32,
    pub message: String,
    pub page_number: usize,
    pub image_index: usize,
    pub rec_results: Vec<RecResult>,
    pub lpls: f64,
}

impl MobileOutput {
    pub fn new(rec_items: &Vec<RecItem>) -> Self {
        let rec_results = rec_items
            .iter()
            .map(|rec_item| RecResult {
                rec_result: vec![false; rec_item.sub_options.len()],
                rec_options: rec_item
                    .sub_options
                    .iter()
                    .map(|coordinate| RecOption {
                        fill_rate: 0.0,
                        coordinate: coordinate.clone(),
                        class_id: 1,
                    })
                    .collect(),
                rec_type: rec_item.rec_type,
            })
            .collect();

        MobileOutput {
            code: 0,
            message: "success".to_string(),
            page_number: 0,
            image_index: 0,
            rec_results,
            lpls: 0.0,
        }
    }
}

/// 整卷标注信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkPaper {
    pub vx_model_path: String,
    pub boundary: Coordinate,
    pub page_number: Vec<Coordinate>,
    pub pages: Vec<MarkPage>,
    #[serde(default = "default_num_threads")]
    pub num_threads: usize,
}

fn default_num_threads() -> usize {
    1
}

impl MarkPaper {
    pub fn is_a4(&self) -> bool {
        self.boundary.w < self.boundary.h
    }

    pub fn resize(&self, scale: f64) -> MarkPaper {
        MarkPaper {
            vx_model_path: self.vx_model_path.clone(),
            boundary: self.boundary.resize(scale),
            page_number: self.page_number.iter().map(|coor| coor.resize(scale)).collect(),
            pages: self.pages.iter().map(|page| page.resize(scale)).collect(),
            num_threads: self.num_threads,
        }
    }

    pub fn init_sort(&mut self) {
        for page in &mut self.pages {
            page.init_sort();
        }
    }
}

/// 单页标注信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkPage {
    pub rec_items: Vec<RecItem>,
    pub assist_location: AssistLocation,
}

impl MarkPage {
    pub fn resize(&self, scale: f64) -> MarkPage {
        MarkPage {
            rec_items: self.rec_items.iter().map(|item| item.resize(scale)).collect(),
            assist_location: self.assist_location.resize(scale),
        }
    }

    fn init_sort(&mut self) {
        self.assist_location.init_sort();
    }
}
