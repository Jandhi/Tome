use log::info;
use openai::{chat::{ChatCompletion, ChatCompletionMessage, ChatCompletionMessageRole}, Credentials};
use schemars::schema_for;
use serde::Deserialize;

pub async fn get_ai_message(system :&str, user:&str) -> String {
    // Relies on OPENAI_KEY and optionally OPENAI_BASE_URL.
    let credentials = Credentials::from_env();
    let messages = vec![
        ChatCompletionMessage {role:ChatCompletionMessageRole::System,content:Some(system.to_string()),name:None,function_call:None, tool_call_id: None, tool_calls: None },
        ChatCompletionMessage {role:ChatCompletionMessageRole::User,content:Some(user.to_string()),name:None,function_call:None, tool_call_id: None, tool_calls: None },
    ];
    let chat_completion = ChatCompletion::builder("gpt-4o", messages.clone())
        .credentials(credentials.clone())
        .create()
        .await
        .unwrap();
    let returned_message = chat_completion.choices.first().unwrap().message.clone();

    let content = returned_message.content.unwrap();
    let string_content = content.trim();
    string_content.to_string()
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
    let response = get_ai_message(&format!("You are a helpful assistant. Format your response in JSON according to the following schema: {}. Do NOT include the schema in teh response.", schema), query).await;

    info!("AI Response: {}", response);

    let json_response = extract_json(&response).unwrap_or(response.to_string());

    serde_json::from_str(&json_response).unwrap_or(None)
}