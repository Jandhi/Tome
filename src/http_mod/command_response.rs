use serde_derive::Deserialize;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct CommandResponse {
    #[serde(default)]
    pub status: i32,
    #[serde(default)]
    pub message: String,
    // The GDMC /command endpoint returns arbitrary JSON here (often nested or
    // arrays, not just string→string), so keep it untyped rather than failing
    // the whole parse — which used to make `give_player_book` return Err even on
    // a successful placement.
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}