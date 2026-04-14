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
