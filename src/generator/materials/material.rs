use std::{collections::HashMap};
use serde_derive::{Serialize, Deserialize};

use crate::{data::Loadable, generator::materials::MaterialFeature, minecraft::{BlockForm, BlockID}};

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
}

impl Loadable<'_, Material, MaterialId> for Material {
    fn get_key(item: &Material) -> MaterialId {
        item.id.clone()
    }

    fn path() -> &'static str {
        &"materials"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MaterialConnections {
    // Shade
    lighter : Option<MaterialId>,
    darker : Option<MaterialId>,
    // Wear
    less_worn : Option<MaterialId>,
    more_worn : Option<MaterialId>,
    // Moisture
    wetter : Option<MaterialId>,
    drier : Option<MaterialId>,
    // Decoration
    more_decorated : Option<MaterialId>,
    less_decorated : Option<MaterialId>,
}