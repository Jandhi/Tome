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

        let user = r#"Generate a minecraft book with a title, author, and 10 pages of content, all about the history of a fictional town.
            Use minecraft's formatting for color and such but ONLY for titles and keywords
            DO NOT USE § codes with section symbols
            DO NOT USE UNICODE ESCAPE CODES
            Instead do formatting using json elements."#;
        let mut book: Book = try_ai_json::<Book>(user).await.expect("Failed to parse AI response");

        let pages: Vec<String> = book.pages.iter().map(|page| format!("'[{}]'", page.iter().map(|text| serde_json::to_string(text).unwrap()).collect::<Vec<_>>().join(",").replace("\'", "\\'").replace("\\n", "\\\\n"))).collect();
        let page_refs = pages.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        let book = provider.give_player_book(&page_refs, &book.title, &book.author)
            .await
            .expect("Failed to give book");

        println!("Book given: {:?}", book);
    }
}