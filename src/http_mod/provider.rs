use crate::{geometry::Rect3D, http_mod::buildarea};

use super::{biome::PositionedBiome, command_response::CommandResponse, entity::{EntityResponse, PositionedEntity}, height_map::HeightMapType, positioned_block::{BlockPlacementResponse, PositionedBlock}};
use anyhow::Ok;
use flate2::read::GzDecoder;
use log::info;
use reqwest_middleware::ClientBuilder;
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};


#[derive(Debug, Clone)]
pub struct GDMCHTTPProvider {
    base_url: String,
    client: reqwest_middleware::ClientWithMiddleware,
}

impl GDMCHTTPProvider {
    pub fn new() -> Self {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let client = ClientBuilder::new(reqwest::Client::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        GDMCHTTPProvider {
            base_url: "http://localhost:9000".to_string(),
            client: client,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path)
    }

    pub async fn command(&self, commands : Vec<String>) -> anyhow::Result<Vec<CommandResponse>> {
        let url = self.url("command"); 
        let client = &self.client;
        let response= client.post(&url)
            .body(commands.join("\n"))
            .send()
            .await?;
        
        let text = response.text().await?;
        Self::log_response(&text);
        let command_response: Vec<CommandResponse> = serde_json::from_str(&text)?;
        Ok(command_response)
    }

    pub async fn get_blocks(&self, x: i32, y: i32, z: i32, dx: i32, dy: i32, dz: i32) -> anyhow::Result<Vec<PositionedBlock>> {        
        let include_state = true;
        let include_data = true;

        let url = self.url(&format!("blocks?x={}&y={}&z={}&dx={}&dy={}&dz={}&includeState={}&includeData={}", x, y, z, dx, dy, dz, include_state, include_data));
        let response = self.client
            .get(&url)
            .send()
            .await?;

        let text = response.text().await?;
        Self::log_response(&text);
        let blocks: Vec<PositionedBlock> = serde_json::from_str(&text)?;
        Ok(blocks)
    }

    pub async fn put_blocks(&self, blocks : &Vec<PositionedBlock>) -> anyhow::Result<Vec<BlockPlacementResponse>> {
        info!("Placing blocks: {:?}", blocks);
        let url = self.url("blocks");

        let body = serde_json::to_string(&blocks)?;

        let response = self.client
            .put(&url)
            .body(body)
            .send()
            .await?;

        let text = response.text().await?;
        Self::log_response(&text);
        let block_response: Vec<BlockPlacementResponse> = serde_json::from_str(&text)?;
        Ok(block_response)
    }

    pub async fn get_build_area(&self) -> anyhow::Result<Rect3D> {
        let url = self.url("buildarea");
        let response = self.client
            .get(&url)
            .send()
            .await?;

        let text = response.text().await?;
        Self::log_response(&text);
        let buildarea_response : buildarea::BuildAreaResponse = serde_json::from_str(&text)?;
        Ok(buildarea_response.to_rect())
    }

    pub async fn get_heightmap(&self, x: i32, z: i32, dx: i32, dz: i32, height_map_type : HeightMapType) -> anyhow::Result<Vec<Vec<i32>>> {
        let url = self.url(&format!("heightmap?x={}&z={}&dx={}&dz={}&type={}", x, z, dx, dz, height_map_type));
        let response = self.client
            .get(&url)
            .send()
            .await?;

        let text = response.text().await?;
        Self::log_response(&text);
        let heightmap: Vec<Vec<i32>> = serde_json::from_str(&text)?;
        Ok(heightmap)
    }

    pub async fn get_biomes(&self, x: i32, y: i32, z: i32, dx: i32, dy: i32, dz: i32) -> anyhow::Result<Vec<PositionedBiome>> {
        let url = self.url(&format!("biomes?x={}&y={}&z={}&dx={}&dy={}&dz={}", x, y, z, dx, dy, dz));
        let response = self.client
            .get(&url)
            .send()
            .await?;

        let text = response.text().await?;
        Self::log_response(&text);
        let biomes: Vec<PositionedBiome> = serde_json::from_str(&text)?;
        Ok(biomes)
    }

    pub async fn get_chunks(&self, x: i32, y: i32, z: i32, dx: i32, dy: i32, dz: i32) -> anyhow::Result<Vec<PositionedBlock>> {
        let url = self.url(&format!("chunks?x={}&y={}&z={}&dx={}&dy={}&dz={}", x, y, z, dx, dy, dz));
        let response = self.client
            .get(&url)
            .header("Accept-Encoding", "gzip")
            .send()
            .await?;

        let raw_bytes = response.bytes().await?;
        let mut decompressed = GzDecoder::new(&raw_bytes[..]);
        let mut decompressed_bytes = Vec::new();
        std::io::copy(&mut decompressed, &mut decompressed_bytes)?;

        

        todo!("Handle gzip response and then deserialize nbt")
    }

    pub async fn get_entities(&self, x: i32, y: i32, z: i32, dx: i32, dy: i32, dz: i32) -> anyhow::Result<Vec<EntityResponse>> {
        let url = self.url(&format!("entities?x={}&y={}&z={}&dx={}&dy={}&dz={}", x, y, z, dx, dy, dz));
        let response = self.client
            .get(&url)
            .send()
            .await?;

        let text = response.text().await?;
        Self::log_response(&text);
        let entities: Vec<EntityResponse> = serde_json::from_str(&text)?;
        Ok(entities)
    }

    pub async fn put_entities(&self, x: i32, y: i32, z: i32, entities : &Vec<PositionedEntity>) -> anyhow::Result<()> {
        let url = self.url(&format!("entities?x={}&y={}&z={}", x, y, z));

        let body = serde_json::to_string(&entities)?;

        let response = self.client
            .put(&url)
            .body(body)
            .send()
            .await?;

        let text = response.text().await?;
        Self::log_response(&text);
        Ok(())
    }

    fn log_response(text : &str) {
        if text.len() > 1000 {
            info!("Response: {}...", &text[..1000]);
        } else {
            info!("Response: {}", text);
        }
    }

    
}