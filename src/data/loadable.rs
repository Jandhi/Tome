use std::{collections::HashMap, env, fs::{read_dir, File}, path::Path};

use log::{info, warn};
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
        Ok(items)
    }

    fn load_all_in(path: &Path, items: &mut HashMap<TKey, TItem>) -> anyhow::Result<()> {
        for entry in read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                Self::load_all_in(&entry_path, items)?;
            } else if entry_path.extension().is_some_and(|ext| ext == "json") {
                let file = File::open(&entry_path)?;
                let item: TItem = match serde_json::from_reader(file) {
                    Ok(val) => val,
                    Err(err) => {
                        warn!("Failed to deserialize {:?}: {}", entry_path, err);
                        continue;
                    }
                };
                let key = Self::get_key(&item);
                items.insert(key, item);
            }
        }
        Ok(())
    }

    fn get_key(item: &TItem) -> TKey;

    // In case we need to do something after loading all items
    fn post_load(items : &mut HashMap<TKey, TItem>) -> anyhow::Result<()>;
    
    fn path() -> &'static str;
}
