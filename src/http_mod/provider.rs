use std::io::Read;

use crate::{geometry::Rect3D, http_mod::buildarea, minecraft::{Chunk, Chunks}};

use super::{biome::PositionedBiome, command_response::CommandResponse, entity::{EntityResponse, PositionedEntity}, height_map::HeightMapType, positioned_block::{BlockPlacementResponse, PositionedBlock}};
use anyhow::Ok;
use flate2::read::GzDecoder;
use log::{debug, info};
use reqwest_middleware::ClientBuilder;
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};

const PROVIDER_LOG_LIMIT : usize = 300;


#[derive(Debug, Clone)]
pub struct GDMCHTTPProvider {
    base_url: String,
    client: reqwest_middleware::ClientWithMiddleware,
    log_responses: bool,
}

impl GDMCHTTPProvider {
    pub fn new() -> Self {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let client = ClientBuilder::new(reqwest::Client::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        GDMCHTTPProvider {
            base_url: "http://localhost:9000".to_string(),
            client,
            log_responses: false,
        }
    }

    pub fn with_response_logging(mut self, log_responses: bool) -> Self {
        self.log_responses = log_responses;
        self
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
        self.log_response(&text);
        let command_response: Vec<CommandResponse> = serde_json::from_str(&text)?;
        Ok(command_response)
    }

    pub async fn give_player_book(&self, pages : &Vec<&str>, title : &str, author : &str) -> anyhow::Result<CommandResponse> {
        let pages_json = pages.join(",");

        let command = format!("give @a written_book[{{pages:[{}],title:\"{}\",author:\"{}\"}}]", pages_json, title, author);

        println!("Command: {}", command);

        Ok(self.command(vec![command]).await?[0].clone())
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
        self.log_response(&text);
        let blocks: Vec<PositionedBlock> = serde_json::from_str(&text)?;
        Ok(blocks)
    }

    pub async fn put_blocks(&self, blocks : &Vec<PositionedBlock>) -> anyhow::Result<Vec<BlockPlacementResponse>> {
        let url = self.url("blocks");

        let body = serde_json::to_string(&blocks)?;
        info!("Sending PUT request to {} with body: {}", url, body);
        let response = self.client
            .put(&url)
            .body(body)
            .send()
            .await?;

        let text = response.text().await?;
        self.log_response(&text);
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
        self.log_response(&text);
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
        self.log_response(&text);
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
        self.log_response(&text);
        let biomes: Vec<PositionedBiome> = serde_json::from_str(&text)?;
        Ok(biomes)
    }

     pub async fn get_chunks(&self, x: i32, y: i32, z: i32, dx: i32, dy: i32, dz: i32) -> anyhow::Result<Vec<Chunk>> {
        let url = self.url(&format!("chunks?x={}&y={}&z={}&dx={}&dy={}&dz={}", x, y, z, dx, dy, dz));
        let response = self.client
            .get(&url)
            .header("Accept-Encoding", "gzip")
            .send()
            .await?;

        let raw_bytes = response.bytes().await?;
        let mut decoder = GzDecoder::new(&raw_bytes[..]);
        let mut buf = vec![];
        decoder.read_to_end(&mut buf)?;
        debug!("Decompressed {} bytes from chunk data", buf.len());

        if let std::result::Result::Ok(chunks) = fastnbt::from_bytes::<Chunks>(&buf) {
            debug!("Decompressed NBT value: {:?}", chunks);
            return Ok(chunks.chunks);
        }

        let mut decoder = GzDecoder::new(buf.as_slice());
        let mut buf = vec![];
        decoder.read_to_end(&mut buf)?;
        debug!("Decompressed {} bytes from NBT data", buf.len());

        let chunks : Chunks = fastnbt::from_bytes(&buf)?;
        debug!("Decompressed NBT value: {:?}", chunks);

        Ok(chunks.chunks)
    }


    pub async fn get_entities(&self, x: i32, y: i32, z: i32, dx: i32, dy: i32, dz: i32) -> anyhow::Result<Vec<EntityResponse>> {
        let url = self.url(&format!("entities?x={}&y={}&z={}&dx={}&dy={}&dz={}", x, y, z, dx, dy, dz));
        let response = self.client
            .get(&url)
            .send()
            .await?;

        let text = response.text().await?;
        self.log_response(&text);
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
        self.log_response(&text);
        Ok(())
    }

    fn log_response(&self, text : &str) {
        if !self.log_responses {
            return;
        }

        if text.len() > PROVIDER_LOG_LIMIT {
            info!("Response: {}...", &text[..PROVIDER_LOG_LIMIT]);
        } else {
            info!("Response: {}", text);
        }
    }

    
}