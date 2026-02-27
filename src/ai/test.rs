#[cfg(test)]
mod tests {
    use crate::{ai::ai::{extract_json, get_ai_message, try_ai_json}, http_mod::GDMCHTTPProvider, util::{init_logger, json_escape}};
    use dotenv::dotenv;
    use schemars::{schema_for, JsonSchema};
    use serde::Serialize;
    use serde_json::Value;

    #[tokio::test]
    pub async fn test_ai() {
        dotenv().ok();

        let system = "You are a helpful assistant.";
        let user = "Generate a town name that sounds japanese, as well as a description of the town. Make it a JSON string with keys 'name' and 'description'.";
        let message = get_ai_message(system, user).await;

        println!("AI Response: {}", extract_json(&message).unwrap_or("No JSON found".to_string()));
    }

    #[tokio::test]
    pub async fn schema() {
        dotenv().ok();

        #[derive(Debug, serde_derive::Deserialize, JsonSchema)]
        enum TownType {
            Village,
            City,
            Metropolis,
        }

        #[derive(Debug, serde_derive::Deserialize, JsonSchema)]
        struct Town {
            name: String,
            town_type : TownType,
            description: String,
        }

        let user = "Generate a town name that sounds japanese, as well as a description of the town.";
        let obj = try_ai_json::<Town>(user).await;

        println!("AI Response: {:?}", obj);
    }

    #[tokio::test]
    async fn test_give_book() {
        dotenv().ok();
        init_logger();
        let provider = GDMCHTTPProvider::new();

        #[derive(Debug, serde_derive::Deserialize, Serialize, JsonSchema)]
        struct Text {
            text: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            color: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            bold: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            italic: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            underlined: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            strikethrough: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            obfuscated: Option<bool>,
        }
        
        #[derive(Debug, serde_derive::Deserialize, JsonSchema)]
        struct Book {
            title: String,
            author : String,
            pages : Vec<Vec<Text>>,
        }

        let user = r#"Generate a minecraft book with a title, author, and 10 pages of content, about redstone.
            Only use color formatting or bold for keywords or titles. Leave most of the body text in plain format.
            DO NOT USE § codes with section symbols
            DO NOT USE UNICODE ESCAPE CODES
            Instead do formatting using json elements."#;
        let book: Book = try_ai_json::<Book>(user).await.expect("Failed to parse AI response");

        let pages: Vec<String> = book.pages.iter().map(|page| {
            let mut components = page.iter();
            if let Some(first) = components.next() {
                let mut first_obj = serde_json::to_value(first).unwrap();
                if let Some(first_map) = first_obj.as_object_mut() {
                    let extra: Vec<serde_json::Value> = components
                        .map(|t| serde_json::to_value(t).unwrap())
                        .collect();
                    if !extra.is_empty() {
                        first_map.insert("extra".to_string(), serde_json::Value::Array(extra));
                    }
                }

                // Serialize and escape as a JSON string
                format!("'{}'", (&first_obj.to_string().replace("\'", "\\\'"))).replace("\n", "\\\\n").replace("\\n", "\\\\n")
            } else {
                "\"\"".to_string() // Empty string for blank pages
            }
        }).collect();
        let page_refs = pages.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        let book = provider.give_player_book(&page_refs, &book.title, &book.author)
            .await
            .expect("Failed to give book");

        println!("Book given: {:?}", book);
    }
}