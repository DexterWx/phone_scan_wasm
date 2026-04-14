use anyhow::{Result, Context};
use serde_json;
use std::any::type_name;

pub fn from_json<T>(json_str: &str) -> Result<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    serde_json::from_str(json_str)
        .context(format!("{} 反序列化失败", type_name::<T>()))
}

pub fn to_json<T>(value: &T) -> Result<String>
where
    T: serde::Serialize,
{
    serde_json::to_string(value)
        .context(format!("{} 序列化失败", type_name::<T>()))
}
