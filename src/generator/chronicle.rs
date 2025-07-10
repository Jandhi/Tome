use std::collections::HashMap;
use log::error;
use schemars::JsonSchema;
use serde_derive::{Serialize, Deserialize};

use crate::{ai::try_ai_json, editor::{self, Editor, World}, minecraft::Biome};

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
                .map(|(biome, _)| **biome)
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

pub async fn give_player_book(editor : &Editor, instruction : &str) {
    let user = &format!(r#"{}.
            Only use color formatting or bold for keywords or titles. Leave most of the body text in plain format.
            DO NOT USE § codes with section symbols
            DO NOT USE UNICODE ESCAPE CODES
            Instead do formatting using json elements."#, instruction);
    let book: Book = try_ai_json::<Book>(user).await.expect("Failed to parse AI response");

    let pages: Vec<String> = book.pages.iter().map(|page| format!("'[{}]'", page.iter().map(|text| serde_json::to_string(text).unwrap()).collect::<Vec<_>>().join(",").replace("\'", "\\'").replace("\\n", "\\\\n"))).collect();
    let page_refs = pages.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    if let Err(e) = editor.give_player_book(&page_refs, &book.title, &book.author).await {
        error!("Error while giving player book: {:?}", e);
    }
}

pub async fn generate_chronicle(editor: &Editor) {
    let world = editor.world();
    let settlement_info = SettlementInfo::new(world);
    
    let mut settlement_info = settlement_info;
    settlement_info.generate_name().await;

    let instruction = format!("Generate a long book about a settlement named '{}', which has {} houses and the following most common biomes: {:?}.", 
                              settlement_info.name, 
                              settlement_info.house_count, 
                              settlement_info.top_three_biomes);

    give_player_book(editor, &instruction).await;
}