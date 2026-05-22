use std::collections::HashMap;
use anyhow::Ok;
use log::{error, info};
use schemars::JsonSchema;
use serde_derive::{Serialize, Deserialize};

use crate::{ai::try_ai_json, editor::{Editor, World}, minecraft::Biome};

pub struct SettlementInfo {
    pub name : String,
    pub top_three_biomes : Vec<Biome>,
    pub house_count : usize,
}

impl SettlementInfo {
    pub fn new(world : &World) -> Self {
        let mut biomes_by_count = world.district_analysis_data.iter()
                .map(|(_, data)| data.biome_count())
                .flatten()
                .fold(HashMap::new(), |mut value, (biome, count)| {
                    value.entry(biome)
                        .and_modify(|e : &mut u32| *e += count)
                        .or_insert(*count);
                    value
                })
                .into_iter()
                .collect::<Vec<_>>();
        biomes_by_count.sort_by_key(|(_, count)| *count);
        
        SettlementInfo {
            name : "".to_string(),
            top_three_biomes: biomes_by_count.iter()
                .rev()
                .take(3)
                .map(|(biome, _)| (*biome).clone())
                .collect(),
            house_count : world.buildings.len(),
        }
    }

    pub async fn generate_name(&mut self) {
        #[derive(Debug, Serialize, Deserialize, JsonSchema)]
        pub struct NameQuery {
            name : String
        }

        try_ai_json::<NameQuery>(&format!("Generate a name for a settlement with {} houses, and the following most common biomes: {:?}.", self.house_count, self.top_three_biomes)).await
            .map(|query| {
                self.name = query.name;
            })
            .unwrap_or_else(|| {
                error!("Failed to generate settlement name, using default.");
                self.name = "Blackbarrow".to_string();
            });
    }
}

#[derive(Debug, serde_derive::Deserialize, Serialize, JsonSchema)]
struct Text {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<TextColors>,
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

#[derive(Debug, serde_derive::Deserialize, Serialize, JsonSchema)]
enum TextColors {
    #[serde(rename = "dark_blue")]
    DarkBlue,
    #[serde(rename = "dark_green")]
    DarkGreen,
    #[serde(rename = "dark_aqua")]
    DarkAqua,
    #[serde(rename = "dark_red")]
    DarkRed,
    #[serde(rename = "dark_purple")]
    DarkPurple,
    #[serde(rename = "gold")]
    Gold,
    #[serde(rename = "gray")]
    Gray,
    #[serde(rename = "dark_gray")]
    DarkGray,
    #[serde(rename = "blue")]
    Blue,
    #[serde(rename = "green")]
    Green,
    #[serde(rename = "aqua")]
    Aqua,
    #[serde(rename = "red")]
    Red,
    #[serde(rename = "light_purple")]
    LightPurple,
    #[serde(rename = "yellow")]
    Yellow,
    #[serde(rename = "white")]
    White,
    #[serde(rename = "black")]
    #[serde(other)]
    Black,
}

#[derive(Debug, serde_derive::Deserialize, JsonSchema)]
struct Book {
    title: String,
    author : String,
    pages : Vec<Vec<Text>>,
}

pub async fn give_player_book(editor : &Editor, instruction : &str) -> anyhow::Result<()> {
    let user = &format!(r#"{}.
            Use Color and Bold ONLY for KEYWORDS. Most of the body should not be colored or bolded. Leave most of the body text in plain format.
            Please limit the title and author to less than 32 characters each.
            DO NOT USE § codes with section symbols
            DO NOT USE UNICODE ESCAPE CODES
            Instead do formatting using json elements."#, instruction);
    let book: Book = try_ai_json::<Book>(user).await
        .ok_or_else(|| anyhow::anyhow!("Failed to get or parse AI response for book"))?;

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
                format!("'{}'", (&first_obj.to_string().replace("\'", "\\\'")))
                    .replace("\n", "\\\\n")
                    .replace("\\n", "\\\\n") // Only double escaped newlines allowed
                    .replace("\\\"", "\"") // We don't want to escape quotes
            } else {
                "\"\"".to_string() // Empty string for blank pages
            }
        }).collect();
    let page_refs = pages.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    if let Err(e) = editor.give_player_book(&page_refs, &book.title, &book.author).await {
        error!("Error while giving player book: {:?}", e);
        return Err(e);
    }

    Ok(())
}

pub async fn generate_chronicle(editor: &Editor, settlement_info : &mut SettlementInfo) -> anyhow::Result<()> {
    let retries = 3;
    settlement_info.generate_name().await;

    for _ in 0..retries {
        let instruction = format!("Generate a long book about a settlement named '{}', which has {} houses and the following most common biomes: {:?}.", 
                              settlement_info.name, 
                              settlement_info.house_count, 
                              settlement_info.top_three_biomes);

        let result = give_player_book(editor, &instruction).await;

        match result {
            Result::Ok(()) => {
                println!("Chronicle generated successfully.");
                return anyhow::Ok(());
            },
            Err(e) => {
                if format!("{:?}", e).contains("sequence, expected a string") {
                    info!("Chronicle generated successfully.");
                    return Ok(());
                }
                
                error!("Error generating chronicle: {:?}, retrying", e);
            }
        }
    }

    Err(anyhow::anyhow!("Failed to generate chronicle after {} retries", retries))
}