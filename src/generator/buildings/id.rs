#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BuildingID(pub usize);

impl From<usize> for BuildingID {
    fn from(id: usize) -> Self {
        BuildingID(id)
    }
}