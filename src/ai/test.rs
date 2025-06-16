#[cfg(test)]
mod tests {
    use crate::{ai::ai::{extract_json, get_ai_message, try_ai_json}, http_mod::GDMCHTTPProvider, util::{init_logger, json_escape}};
    use dotenv::dotenv;
    use schemars::{schema_for, JsonSchema};
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
        
        #[derive(Debug, serde_derive::Deserialize, JsonSchema)]
        struct Book {
            title: String,
            author : String,
            pages : Vec<Vec<String>>,
        }

        let user = "Generate a minecraft book with a title, author, and 3 pages of content, all about the evolution of the chrysalis. Make sure to use some of minecraft's pretty text effects in the json, but don't go overboard! Don't use § codes or weird unicode, instead do formatting in json, but make sure its all in a nice json escaped string.";
        let mut book: Book = try_ai_json::<Book>(user).await.expect("Failed to parse AI response");

        let pages: Vec<String> = book.pages.iter().map(|s| format!("[{}]", s.join(",")).replace("\\n", "\\\\n")).collect();
        let page_refs = pages.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        let book = provider.give_player_book(&page_refs, &book.title, &book.author)
            .await
            .expect("Failed to give book");

        println!("Book given: {:?}", book);
    }
}