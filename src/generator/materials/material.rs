use std::{collections::HashMap};
use anyhow::Ok;
use serde_derive::{Serialize, Deserialize};

use crate::{data::Loadable, editor::Editor, generator::materials::{feature::{map_features, MaterialParameters}, MaterialFeature}, geometry::Point3D, minecraft::{Block, BlockForm, BlockID}};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MaterialId(String);

impl MaterialId {
    pub fn new(id: String) -> Self {
        MaterialId(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Material {
    id : MaterialId,
    connections : Option<MaterialConnections>,
    blocks : HashMap<BlockForm, BlockID>,
}

impl Material {
    pub fn id(&self) -> &MaterialId {
        &self.id
    }

    pub fn more(&self, feature: MaterialFeature) -> Option<&MaterialId> {
        self.connections.as_ref().and_then(|connections| match feature {
            MaterialFeature::Shade => connections.lighter.as_ref(),
            MaterialFeature::Wear => connections.less_worn.as_ref(),
            MaterialFeature::Moisture => connections.wetter.as_ref(),
            MaterialFeature::Decoration => connections.more_decorated.as_ref(),
        })
    }

    pub fn less(&self, feature: MaterialFeature) -> Option<&MaterialId> {
        self.connections.as_ref().and_then(|connections| match feature {
            MaterialFeature::Shade => connections.darker.as_ref(),
            MaterialFeature::Wear => connections.more_worn.as_ref(),
            MaterialFeature::Moisture => connections.drier.as_ref(),
            MaterialFeature::Decoration => connections.less_decorated.as_ref(),
        })
    }

    pub fn get_block(&self, form: &BlockForm) -> Option<&BlockID> {
        self.blocks.get(form)
    }

    pub async fn place_block(&self, editor : &mut Editor, point : Point3D, form : BlockForm, materials : &HashMap<MaterialId, Material>, states : Option<HashMap<String, String>>, data : Option<String>, parameters : MaterialParameters) {
        let material = map_features(&parameters, self.id(), materials);
        
        if let Some(block_id) = materials.get(&material).unwrap().get_block(&form) {
            editor.place_block(&Block{
                id: block_id.clone(),
                state: states,
                data,
            }, point).await;
        } else {
            log::warn!("No block found for material {} with form {:?}", self.id().0, form);
        }
    }
}

impl Loadable<'_, Material, MaterialId> for Material {
    fn get_key(item: &Material) -> MaterialId {
        item.id.clone()
    }

    fn path() -> &'static str {
        &"materials"
    }
    
    fn post_load(_items : &mut HashMap<MaterialId, Material>) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MaterialConnections {
    // Shade
    lighter: Option<MaterialId>,
    darker: Option<MaterialId>,
    // Wear
    less_worn: Option<MaterialId>,
    more_worn: Option<MaterialId>,
    // Moisture
    wetter: Option<MaterialId>,
    drier: Option<MaterialId>,
    // Decoration
    more_decorated: Option<MaterialId>,
    less_decorated: Option<MaterialId>,
}
