use std::{collections::HashMap};
use anyhow::Ok;
use serde_derive::{Serialize, Deserialize};

use crate::{data::Loadable,minecraft::{Block, BlockForm, BlockID}, generator::terrain::Tree};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ForestId(String);

impl ForestId {
    pub fn new(id: String) -> Self {
        ForestId(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Forest {
    id : ForestId,
    trees: HashMap<Tree, f32>,
    tree_palette: HashMap<Tree, HashMap<String, HashMap<String, f32>>>,
    tree_density: u32,
}

impl Forest {
    pub fn id(&self) -> &ForestId {
        &self.id
    }

    pub fn trees(&self) -> &HashMap<Tree, f32> {
        &self.trees
    }

    pub fn tree_palette(&self) -> &HashMap<Tree, HashMap<String, HashMap<String, f32>>> {
        &self.tree_palette
    }

    pub fn tree_density(&self) -> u32 {
        self.tree_density
    }   
}

impl Loadable<'_, Forest, ForestId> for Forest {
    fn get_key(item: &Forest) -> ForestId {
        item.id.clone()
    }

    fn path() -> &'static str {
        &"forests"
    }

    fn post_load(_items : &mut HashMap<ForestId, Forest>) -> anyhow::Result<()> {
        Ok(())
    }
}