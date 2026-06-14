use log::info;
use openai::{chat::{ChatCompletion, ChatCompletionMessage, ChatCompletionMessageRole, ChatCompletionResponseFormat}, Credentials};
use schemars::schema_for;
use serde::Deserialize;

/// Model used for chronicle naming + lore. `gpt-5-mini` is plenty for this
/// light creative/structured work and ~5x cheaper than the old `gpt-4o`
/// (pennies per settlement at our volume). Override with `OPENAI_MODEL`.
const DEFAULT_MODEL: &str = "gpt-5-mini";

fn model() -> String {
    std::env::var("OPENAI_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string())
}

/// Core chat call. With `json_mode`, asks the API for a single valid JSON object
/// (no prose, no fenced block) — the OPENAI requires the word "json" in the
/// prompt, which the JSON callers include.
async fn chat(system: &str, user: &str, json_mode: bool) -> anyhow::Result<String> {
    // Relies on OPENAI_KEY and optionally OPENAI_BASE_URL.
    let credentials = Credentials::from_env();
    let messages = vec![
        ChatCompletionMessage {role:ChatCompletionMessageRole::System,content:Some(system.to_string()),name:None,function_call:None, tool_call_id: None, tool_calls: None },
        ChatCompletionMessage {role:ChatCompletionMessageRole::User,content:Some(user.to_string()),name:None,function_call:None, tool_call_id: None, tool_calls: None },
    ];
    let mut builder = ChatCompletion::builder(&model(), messages.clone())
        .credentials(credentials.clone());
    if json_mode {
        builder = builder.response_format(ChatCompletionResponseFormat::json_object());
    }
    let chat_completion = builder.create().await?;
    let returned_message = chat_completion.choices.first()
        .ok_or_else(|| anyhow::anyhow!("No choices returned from AI"))?.message.clone();

    let content = returned_message.content
        .ok_or_else(|| anyhow::anyhow!("AI returned empty content"))?;
    let string_content = content.trim();
    Ok(string_content.to_string())
}

pub async fn get_ai_message(system :&str, user:&str) -> anyhow::Result<String> {
    chat(system, user, false).await
}

pub fn extract_json(response : &str) -> Option<String> {
    let begin = response.find("```json");
    let end = response.rfind("```");

    if let (Some(b), Some(e)) = (begin, end) {
        if b < e {
            let json_content = &response[b + 7..e].trim(); // Skip "```json" and "```"
            return Some(json_content.to_string());
        }
    }

    None
}

pub async fn try_ai_json<T>(query : &str) -> Option<T>
where T: for<'de> Deserialize<'de> + schemars::JsonSchema {
    let schema = serde_json::to_string_pretty(&schema_for!(T)).unwrap();
    // JSON mode makes the API return a single valid JSON object directly (no
    // prose, no fenced block), so we can parse the response as-is.
    let response = match chat(&format!("You are a helpful assistant. Format your response as a JSON object matching this schema: {}. Do NOT include the schema in the response.", schema), query, true).await {
        Ok(r) => r,
        Err(e) => {
            log::error!("AI request failed: {e}");
            return None;
        }
    };

    info!("AI Response: {}", response);

    // The response should already be raw JSON; fall back to stripping a fenced
    // block just in case a model wraps it anyway.
    let json_response = extract_json(&response).unwrap_or_else(|| response.to_string());

    serde_json::from_str(&json_response).unwrap_or(None)
}