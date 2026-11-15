use std::str::FromStr;

use super::EnumTrait;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DataValidationErrorStyleValues {
    Stop,
    Warning,
    Information,
}

impl EnumTrait for DataValidationErrorStyleValues {
    #[inline]
    fn value_string(&self) -> &str {
        match self {
            Self::Stop => "stop",
            Self::Warning => "warning",
            Self::Information => "information",
        }
    }
}

impl Default for DataValidationErrorStyleValues {
    #[inline]
    fn default() -> Self {
        Self::Stop
    }
}

impl FromStr for DataValidationErrorStyleValues {
    type Err = ();

    #[inline]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(match input {
            "stop" => Self::Stop,
            "warning" => Self::Warning,
            "information" => Self::Information,
            _ => return Err(()),
        })
    }
}
