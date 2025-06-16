#[cfg(test)]
mod tests {
    use crate::ai::ai::{extract_json, get_ai_message, try_ai_json};
    use dotenv::dotenv;
    use schemars::{schema_for, JsonSchema};

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
}