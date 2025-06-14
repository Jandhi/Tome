use serde_derive::{Serialize, Deserialize};

use crate::generator::nbts::Structure;

#[derive(Serialize, Deserialize)]
pub struct Roof {
    #[serde(flatten)]
    structure : Structure,
}

