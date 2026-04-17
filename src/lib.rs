pub mod myutils;
pub mod models;
pub mod recognize;
pub mod config;

#[cfg(test)]
mod tests {
    use std::fs;
    use crate::myutils::myjson::to_json;
    use crate::myutils::image::imread;
    use crate::recognize::engine;
    use anyhow::Result;

    #[test]
    fn test_paper() -> Result<()> {
        let scan_id = "270715";
        let scan_path = format!("dev/test_data/cards/{scan_id}/test.json");
        let img_path = format!("dev/test_data/cards/{scan_id}/test.jpg");
        let image = imread(&img_path)?;

        let scan_string = fs::read_to_string(scan_path)?;

        let engine = engine::RecEngine::new(&scan_string)?;
        let (res, _rgb) = engine.inference(&image)?;
        println!("lpls: {}", res.lpls);

        fs::write(format!("dev/test_data/out/{scan_id}.json"), to_json(&res)?)?;

        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
mod build {
    use wasm_bindgen::prelude::*;
    use std::sync::Mutex;
    use once_cell::sync::Lazy;
    use crate::recognize::engine::RecEngine;
    use crate::myutils::myjson::to_json;
    use crate::models::MobileOutput;

    // 全局引擎实例
    static ENGINE: Lazy<Mutex<Option<RecEngine>>> = Lazy::new(|| Mutex::new(None));

    /// WASM 推理结果
    #[wasm_bindgen]
    pub struct WasmInferenceResult {
        json: String,
        image_data: Vec<u8>,
        width: u32,
        height: u32,
    }

    #[wasm_bindgen]
    impl WasmInferenceResult {
        /// 获取 JSON 结果
        #[wasm_bindgen(getter)]
        pub fn json(&self) -> String {
            self.json.clone()
        }

        /// 获取图片数据（RGB 格式）
        #[wasm_bindgen(getter)]
        pub fn image_data(&self) -> Vec<u8> {
            self.image_data.clone()
        }

        /// 获取图片宽度
        #[wasm_bindgen(getter)]
        pub fn width(&self) -> u32 {
            self.width
        }

        /// 获取图片高度
        #[wasm_bindgen(getter)]
        pub fn height(&self) -> u32 {
            self.height
        }
    }

    fn make_failed_result(message: String) -> WasmInferenceResult {
        let output = MobileOutput {
            code: 1,
            message,
            page_number: 0,
            image_index: 0,
            rec_results: vec![],
            lpls: 0.0,
        };
        WasmInferenceResult {
            json: to_json(&output).unwrap_or_else(|_| "{}".to_string()),
            image_data: vec![],
            width: 0,
            height: 0,
        }
    }

    fn do_inference(image: &image::DynamicImage) -> WasmInferenceResult {
        let engine_lock = match ENGINE.lock() {
            Ok(lock) => lock,
            Err(e) => return make_failed_result(format!("获取引擎锁失败: {}", e)),
        };

        let engine = match engine_lock.as_ref() {
            Some(e) => e,
            None => return make_failed_result("请先调用 init_engine 初始化引擎".to_string()),
        };

        let (output, rgb) = match engine.inference(image) {
            Ok(result) => result,
            Err(e) => return make_failed_result(format!("识别失败: {}", e)),
        };

        let json = match to_json(&output) {
            Ok(j) => j,
            Err(e) => return make_failed_result(format!("JSON 序列化失败: {}", e)),
        };

        let width = rgb.width();
        let height = rgb.height();
        let image_data = rgb.into_raw();

        WasmInferenceResult { json, image_data, width, height }
    }

    /// 初始化引擎
    ///
    /// 参数:
    /// - scan_json: 扫描配置 JSON 字符串
    ///
    /// 返回:
    /// - JSON 字符串: {"code": 0, "message": "初始化成功"} 或 {"code": 1, "message": "错误信息"}
    #[wasm_bindgen]
    pub fn init_engine(scan_json: &str) -> String {

        let engine = match RecEngine::new(&scan_json.to_string()) {
            Ok(e) => e,
            Err(e) => return format!("{{\"code\":1,\"message\":\"初始化引擎失败: {}\"}}", e),
        };

        let mut engine_lock = match ENGINE.lock() {
            Ok(lock) => lock,
            Err(e) => return format!("{{\"code\":1,\"message\":\"获取引擎锁失败: {}\"}}", e),
        };

        *engine_lock = Some(engine);
        "{\"code\":0,\"message\":\"初始化成功\"}".to_string()
    }

    /// 从 RGBA 原始帧数据推理（小程序 CameraFrame）
    ///
    /// 参数:
    /// - rgba_data: RGBA 像素数据 (Uint8Array)，长度应为 width * height * 4
    /// - width: 图片宽度
    /// - height: 图片高度
    #[wasm_bindgen]
    pub fn inference_from_rgba(rgba_data: &[u8], width: u32, height: u32) -> WasmInferenceResult {
        let expected_len = (width * height * 4) as usize;
        if rgba_data.len() != expected_len {
            return make_failed_result(format!(
                "RGBA 数据长度不匹配: 期望 {} ({}x{}x4), 实际 {}",
                expected_len, width, height, rgba_data.len()
            ));
        }

        let rgba_image = match image::RgbaImage::from_raw(width, height, rgba_data.to_vec()) {
            Some(img) => img,
            None => return make_failed_result("无法从 RGBA 数据创建图片".to_string()),
        };

        let image = image::DynamicImage::ImageRgba8(rgba_image);
        do_inference(&image)
    }
}
