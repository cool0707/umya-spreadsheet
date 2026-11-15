use std::str::FromStr;

use super::EnumTrait;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DataValidationImeModeValues {
    NoControl,
    Off,
    On,
    Disabled,
    Hiragana,
    FullKatakana,
    HalfKatakana,
    FullAlpha,
    HalfAlpha,
    FullHangul,
    HalfHangul,
}

impl EnumTrait for DataValidationImeModeValues {
    #[inline]
    fn value_string(&self) -> &str {
        match self {
            Self::NoControl => "noControl",
            Self::Off => "off",
            Self::On => "on",
            Self::Disabled => "disabled",
            Self::Hiragana => "hiragana",
            Self::FullKatakana => "fullKatakana",
            Self::HalfKatakana => "halfKatakana",
            Self::FullAlpha => "fullAlpha",
            Self::HalfAlpha => "halfAlpha",
            Self::FullHangul => "fullHangul",
            Self::HalfHangul => "halfHangul",
        }
    }
}

impl Default for DataValidationImeModeValues {
    #[inline]
    fn default() -> Self {
        Self::NoControl
    }
}

impl FromStr for DataValidationImeModeValues {
    type Err = ();

    #[inline]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(match input {
            "noControl" => Self::NoControl,
            "off" => Self::Off,
            "on" => Self::On,
            "disabled" => Self::Disabled,
            "hiragana" => Self::Hiragana,
            "fullKatakana" => Self::FullKatakana,
            "halfKatakana" => Self::HalfKatakana,
            "fullAlpha" => Self::FullAlpha,
            "halfAlpha" => Self::HalfAlpha,
            "fullHangul" => Self::FullHangul,
            "halfHangul" => Self::HalfHangul,
            _ => return Err(()),
        })
    }
}
