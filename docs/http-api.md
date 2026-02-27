# HTTP API Communication

Tome communicates with Minecraft through the GDMC HTTP Interface mod. This document describes the HTTP provider implementation and available endpoints.

## Overview

The GDMC HTTP Interface is a Minecraft mod that exposes a REST API for reading and writing world data. Tome uses this API to:

- Read existing blocks and terrain
- Place new blocks
- Query heightmaps and biomes
- Execute Minecraft commands
- Load chunk data

## GDMC HTTP Provider (`src/http_mod/`)

### Structure

```rust
pub struct GDMCHTTPProvider {
    client: reqwest::Client,
    base_url: String,
    build_area: Rect3D,
}
```

### Initialization

```rust
impl GDMCHTTPProvider {
    pub async fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let base_url = "http://localhost:9000".to_string();

        // Fetch build area from server
        let build_area = Self::fetch_build_area(&client, &base_url).await?;

        Ok(Self { client, base_url, build_area })
    }
}
```

## API Endpoints

### GET /blocks

Read blocks from the world.

**Parameters:**
- `x`, `y`, `z` - Position
- `dx`, `dy`, `dz` - Size (optional, for regions)

**Response:** Block data in NBT or text format

```rust
pub async fn get_block(&self, point: Point3D) -> Result<Block> {
    let url = format!(
        "{}/blocks?x={}&y={}&z={}",
        self.base_url, point.x, point.y, point.z
    );

    let response = self.client.get(&url).send().await?;
    // Parse response...
}
```

### PUT /blocks

Place blocks in the world.

**Body:** List of positioned blocks

```rust
pub async fn put_blocks(&self, blocks: &[PositionedBlock]) -> Result<()> {
    let url = format!("{}/blocks", self.base_url);

    let body = blocks.iter()
        .map(|pb| format!(
            "~{} ~{} ~{} {}{}",
            pb.point.x, pb.point.y, pb.point.z,
            pb.block.id,
            pb.block.state_string()
        ))
        .collect::<Vec<_>>()
        .join("\n");

    self.client
        .put(&url)
        .body(body)
        .send()
        .await?;

    Ok(())
}
```

### GET /heightmap

Query terrain heightmaps.

**Parameters:**
- `x`, `z` - Position
- `dx`, `dz` - Size

**Response:** Heightmap data

```rust
pub async fn get_heightmap(&self, rect: Rect2D) -> Result<Vec<Vec<i32>>> {
    let url = format!(
        "{}/heightmap?x={}&z={}&dx={}&dz={}",
        self.base_url,
        rect.origin.x, rect.origin.z,
        rect.size.x, rect.size.z
    );

    let response = self.client.get(&url).send().await?;
    // Parse heightmap data...
}
```

### GET /chunks

Get chunk NBT data.

**Parameters:**
- `x`, `z` - Chunk coordinates
- `dx`, `dz` - Number of chunks

**Response:** Compressed NBT chunk data

```rust
pub async fn get_chunks(&self, chunk_x: i32, chunk_z: i32) -> Result<Vec<Chunk>> {
    let url = format!(
        "{}/chunks?x={}&z={}&dx=1&dz=1",
        self.base_url, chunk_x, chunk_z
    );

    let response = self.client.get(&url).send().await?;
    let bytes = response.bytes().await?;

    // Decompress and parse NBT
    let chunks = parse_chunk_nbt(&bytes)?;
    Ok(chunks)
}
```

### POST /command

Execute Minecraft commands.

**Body:** Command string

```rust
pub async fn run_command(&self, command: &str) -> Result<String> {
    let url = format!("{}/command", self.base_url);

    let response = self.client
        .post(&url)
        .body(command.to_string())
        .send()
        .await?;

    Ok(response.text().await?)
}
```

### GET /biome

Query biome at position.

**Parameters:**
- `x`, `y`, `z` - Position

**Response:** Biome identifier

```rust
pub async fn get_biome(&self, point: Point3D) -> Result<Biome> {
    let url = format!(
        "{}/biome?x={}&y={}&z={}",
        self.base_url, point.x, point.y, point.z
    );

    let response = self.client.get(&url).send().await?;
    let biome_str = response.text().await?;

    Ok(Biome::from_str(&biome_str)?)
}
```

### GET /buildarea

Get the current build area.

**Response:** Build area bounds

```rust
pub async fn get_build_area(&self) -> Result<Rect3D> {
    let url = format!("{}/buildarea", self.base_url);

    let response = self.client.get(&url).send().await?;
    // Parse build area...
}
```

## PositionedBlock

Represents a block with its position:

```rust
pub struct PositionedBlock {
    pub point: Point3D,
    pub block: Block,
}

impl PositionedBlock {
    pub fn to_command_string(&self) -> String {
        format!(
            "~{} ~{} ~{} {}{}",
            self.point.x,
            self.point.y,
            self.point.z,
            self.block.id,
            self.block.state_string()
        )
    }
}
```

## Block State Strings

Block states are formatted as Minecraft command syntax:

```rust
impl Block {
    pub fn state_string(&self) -> String {
        match &self.state {
            Some(state) => {
                let pairs: Vec<String> = state
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect();
                format!("[{}]", pairs.join(","))
            }
            None => String::new(),
        }
    }
}

// Example: "oak_stairs[facing=north,half=bottom]"
```

## Error Handling

The provider includes retry logic for transient failures:

```rust
pub async fn request_with_retry<T, F>(&self, operation: F) -> Result<T>
where
    F: Fn() -> Future<Output = Result<T>>,
{
    let mut attempts = 0;
    let max_attempts = 3;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempts < max_attempts => {
                attempts += 1;
                let delay = Duration::from_millis(100 * 2u64.pow(attempts));
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}
```

## Batching

Blocks are batched to reduce HTTP overhead:

```rust
const BATCH_SIZE: usize = 32;

pub async fn put_blocks_batched(&self, blocks: &[PositionedBlock]) -> Result<()> {
    for chunk in blocks.chunks(BATCH_SIZE) {
        self.put_blocks(chunk).await?;
    }
    Ok(())
}
```

## Configuration

Default configuration:

| Setting | Value |
|---------|-------|
| Base URL | `http://localhost:9000` |
| Timeout | 30 seconds |
| Batch Size | 32 blocks |
| Max Retries | 3 |

## Setup Requirements

1. Install Minecraft with Forge/Fabric
2. Install GDMC HTTP Interface mod
3. Start Minecraft and load a world
4. Set build area with `/setbuildarea` command
5. Run Tome

## Debugging

Enable request logging:

```rust
pub async fn put_blocks(&self, blocks: &[PositionedBlock]) -> Result<()> {
    #[cfg(debug_assertions)]
    println!("Placing {} blocks", blocks.len());

    // ... actual request
}
```

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Connection refused | Minecraft not running | Start Minecraft with GDMC mod |
| 404 Not Found | Wrong endpoint | Check GDMC mod version |
| Timeout | Large request | Reduce batch size |
| Invalid block | Wrong block ID | Use Minecraft block IDs |

## References

- [GDMC HTTP Interface GitHub](https://github.com/Niels-NTG/gdmc_http_interface)
- [Minecraft Block IDs](https://minecraft.wiki/w/Block)
- [NBT Format](https://minecraft.wiki/w/NBT_format)
