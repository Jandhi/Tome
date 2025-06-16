use serde_derive::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style {
    #[serde(rename = "medieval")]
    Medieval,
    #[serde(rename = "japanese")]
    Japanese,
}