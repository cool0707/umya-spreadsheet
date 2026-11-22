use std::str::FromStr;

use super::EnumTrait;
#[derive(Clone, Debug)]
pub enum PaneValues {
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}
impl Default for PaneValues {
    /// What the file means when it does not say (`pane` of a selection,
    /// `activePane` of a pane).
    #[inline]
    fn default() -> Self {
        Self::TopLeft
    }
}
impl EnumTrait for PaneValues {
    #[inline]
    fn value_string(&self) -> &str {
        match &self {
            Self::BottomLeft => "bottomLeft",
            Self::BottomRight => "bottomRight",
            Self::TopLeft => "topLeft",
            Self::TopRight => "topRight",
        }
    }
}
impl FromStr for PaneValues {
    type Err = ();

    #[inline]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "bottomLeft" => Ok(Self::BottomLeft),
            "bottomRight" => Ok(Self::BottomRight),
            "topLeft" => Ok(Self::TopLeft),
            // ("TopRight": as this crate wrote it before.)
            "topRight" | "TopRight" => Ok(Self::TopRight),
            _ => Err(()),
        }
    }
}
