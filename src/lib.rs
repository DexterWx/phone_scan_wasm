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
        let scan_id = "270716";
        let scan_path = format!("dev/test_data/cards/{scan_id}/test.json");
        let img_path = format!("dev/test_data/cards/{scan_id}/test.jpg");
        let image = imread(&img_path)?;

        let scan_string = fs::read_to_string(scan_path)?;

        let engine = engine::RecEngine::new_paper(&scan_string)?;
        let (res, _rgb) = engine.inference_paper(&image)?;
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

    /// 初始化引擎
    ///
    /// 参数:
    /// - scan_json: 扫描配置 JSON 字符串
    ///
    /// 返回:
    /// - 成功返回 Ok(())，失败返回错误信息
    #[wasm_bindgen]
    pub fn init_engine(scan_json: &str) -> Result<(), JsValue> {
        // 设置 panic hook，在浏览器控制台显示更友好的错误信息
        console_error_panic_hook::set_once();

        let engine = RecEngine::new_paper(&scan_json.to_string())
            .map_err(|e| JsValue::from_str(&format!("初始化引擎失败: {}", e)))?;

        let mut engine_lock = ENGINE.lock()
            .map_err(|e| JsValue::from_str(&format!("获取引擎锁失败: {}", e)))?;

        *engine_lock = Some(engine);

        Ok(())
    }

    /// 单张图片推理接口（返回 RGB 图片）
    ///
    /// 参数:
    /// - image_data: 图片数据（支持 JPEG、PNG 等格式）
    ///
    /// 返回:
    /// - WasmInferenceResult: 包含 JSON 结果和 RGB 图片数据
    #[wasm_bindgen]
    pub fn inference_paper_and_return_rgb(image_data: &[u8]) -> Result<WasmInferenceResult, JsValue> {
        // 创建失败输出
        let make_failed_output = |message: String| -> MobileOutput {
            MobileOutput {
                code: 1,
                message,
                page_number: 0,
                image_index: 0,
                rec_results: vec![],
                lpls: 0.0,
            }
        };

        // 检查引擎是否初始化
        let engine_lock = ENGINE.lock()
            .map_err(|e| JsValue::from_str(&format!("获取引擎锁失败: {}", e)))?;

        let engine = engine_lock.as_ref()
            .ok_or_else(|| JsValue::from_str("请先调用 init_engine 初始化引擎"))?;

        // 解码图片
        let image = image::load_from_memory(image_data)
            .map_err(|e| {
                let output = make_failed_output(format!("图片解码失败: {}", e));
                let json = to_json(&output).unwrap_or_else(|_| "{}".to_string());
                JsValue::from_str(&json)
            })?;

        // 执行识别
        let (output, rgb) = engine.inference_paper(&image)
            .map_err(|e| {
                let output = make_failed_output(format!("识别失败: {}", e));
                let json = to_json(&output).unwrap_or_else(|_| "{}".to_string());
                JsValue::from_str(&json)
            })?;

        // 转换 JSON
        let json = to_json(&output)
            .map_err(|e| JsValue::from_str(&format!("JSON 序列化失败: {}", e)))?;

        // 转换 RGB 图片为字节数组
        let width = rgb.width();
        let height = rgb.height();
        let image_data = rgb.into_raw();

        Ok(WasmInferenceResult {
            json,
            image_data,
            width,
            height,
        })
    }
}
