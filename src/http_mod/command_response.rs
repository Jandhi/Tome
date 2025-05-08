use std::collections::HashMap;

use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CommandResponse {
    pub status: i32,
    pub message: String,
    pub data: Option<HashMap<String, String>>,
}