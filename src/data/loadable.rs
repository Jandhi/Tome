use std::{collections::HashMap, env, fs::{read_dir, File}, path::Path};

use log::{debug, info};
use serde::de::DeserializeOwned;

use crate::config::DATA_PATH;

pub trait Loadable<'de, TItem, TKey>
    where TItem: DeserializeOwned,
          TKey: Clone + Eq + std::hash::Hash + 'de
{
    fn load() -> anyhow::Result<HashMap<TKey, TItem>> {
        let path = env::current_dir()?.join("data").join(Self::path());
        info!("Loading items from {:?}", DATA_PATH);
        let mut items = HashMap::new();
        Self::load_all_in(&path, &mut items)?;
        Self::post_load(&mut items)?;
        Ok(items)
    }

    fn load_all_in(path: &Path, items: &mut HashMap<TKey, TItem>) -> anyhow::Result<()> {
        for entry in read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                Self::load_all_in(&entry_path, items)?;
            } else {
                let ext = entry_path.extension().and_then(|e| e.to_str());
                // Deserialization failures are logged at debug level. Known
                // drafts (Japanese wall/roof assets on an older schema) skip
                // silently by default; use RUST_LOG=debug to surface them.
                // `test_data_loads_cleanly` asserts that LoadedData as a whole
                // still succeeds.
                let item: TItem = match ext {
                    Some("json") => {
                        let file = File::open(&entry_path)?;
                        match serde_json::from_reader(file) {
                            Ok(val) => val,
                            Err(err) => {
                                debug!("Failed to deserialize {:?}: {}", entry_path, err);
                                continue;
                            }
                        }
                    }
                    Some("yaml" | "yml") => {
                        let file = File::open(&entry_path)?;
                        match serde_yaml::from_reader(file) {
                            Ok(val) => val,
                            Err(err) => {
                                debug!("Failed to deserialize {:?}: {}", entry_path, err);
                                continue;
                            }
                        }
                    }
                    _ => continue,
                };
                let key = Self::get_key(&item);
                items.insert(key, item);
            }
        }
        Ok(())
    }

    fn get_key(item: &TItem) -> TKey;

    // In case we need to do something after loading all items
    fn post_load(_items : &mut HashMap<TKey, TItem>) -> anyhow::Result<()> { Ok(()) }

    fn path() -> &'static str;
}

/// Load a single YAML file from the data directory, deserializing into `T`.
pub fn load_yaml<T: DeserializeOwned>(relative_path: &str) -> anyhow::Result<T> {
    let path = env::current_dir()?.join("data").join(relative_path);
    info!("Loading YAML from {:?}", path);
    let file = File::open(&path)?;
    Ok(serde_yaml::from_reader(file)?)
}

/// Load all YAML files in a data subdirectory and merge them into one HashMap.
pub fn load_yaml_dir<V: DeserializeOwned>(relative_dir: &str) -> anyhow::Result<HashMap<String, V>> {
    let dir = env::current_dir()?.join("data").join(relative_dir);
    info!("Loading YAML dir {:?}", dir);
    let mut merged = HashMap::new();
    for entry in read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        match ext {
            Some("yaml" | "yml") => {
                let file = File::open(&path)?;
                let map: HashMap<String, V> = serde_yaml::from_reader(file)?;
                info!("  Loaded {} items from {:?}", map.len(), path);
                merged.extend(map);
            }
            _ => continue,
        }
    }
    Ok(merged)
}
