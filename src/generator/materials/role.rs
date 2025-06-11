use strum_macros::EnumIter;

#[derive(Clone, Copy, PartialEq, Eq, EnumIter, Debug)]
pub enum MaterialRole {
    PrimaryStone,
    SecondaryStone,
    PrimaryWood,
    SecondaryWood,
    Accent,
}