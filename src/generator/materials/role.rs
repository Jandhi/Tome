use strum_macros::EnumIter;

#[derive(Clone, Copy, PartialEq, Eq, EnumIter)]
pub enum MaterialRole {
    PrimaryStone,
    SecondaryStone,
    PrimaryWood,
    SecondaryWood,
    Accent,
}