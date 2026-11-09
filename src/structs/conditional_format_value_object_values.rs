use std::str::FromStr;

use super::EnumTrait;
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConditionalFormatValueObjectValues {
    /// x14: the automatic minimum of a data bar.
    AutoMin,
    /// x14: the automatic maximum of a data bar.
    AutoMax,
    Formula,
    Max,
    Min,
    Number,
    Percent,
    Percentile,
}
impl Default for ConditionalFormatValueObjectValues {
    #[inline]
    fn default() -> Self {
        Self::Number
    }
}
impl EnumTrait for ConditionalFormatValueObjectValues {
    #[inline]
    fn value_string(&self) -> &str {
        match &self {
            Self::AutoMin => "autoMin",
            Self::AutoMax => "autoMax",
            Self::Formula => "formula",
            Self::Max => "max",
            Self::Min => "min",
            Self::Number => "num",
            Self::Percent => "percent",
            Self::Percentile => "percentile",
        }
    }
}
impl FromStr for ConditionalFormatValueObjectValues {
    type Err = ();

    #[inline]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "autoMin" => Ok(Self::AutoMin),
            "autoMax" => Ok(Self::AutoMax),
            "formula" => Ok(Self::Formula),
            "max" => Ok(Self::Max),
            "min" => Ok(Self::Min),
            "num" => Ok(Self::Number),
            "percent" => Ok(Self::Percent),
            "percentile" => Ok(Self::Percentile),
            _ => Err(()),
        }
    }
}
