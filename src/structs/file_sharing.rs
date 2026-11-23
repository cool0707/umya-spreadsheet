//! The version of the application that wrote the workbook (`fileVersion`)
//! and the sharing of the file (`fileSharing`): Excel's "General Options" of
//! "Save As" (the password to modify, read-only recommended).

use std::io::Cursor;

use quick_xml::{
    Writer,
    events::BytesStart,
};

use crate::writer::driver::write_start_tag;

/// The version of the application that wrote the workbook (`fileVersion`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileVersion {
    app_name:      Option<String>,
    last_edited:   Option<String>,
    lowest_edited: Option<String>,
    rup_build:     Option<String>,
    code_name:     Option<String>,
}

impl FileVersion {
    /// What Excel writes (`appName="xl" lastEdited="5" lowestEdited="4"
    /// rupBuild="9302"`), which a new workbook is written with.
    #[must_use]
    pub fn excel() -> Self {
        Self {
            app_name:      Some("xl".into()),
            last_edited:   Some("5".into()),
            lowest_edited: Some("4".into()),
            rup_build:     Some("9302".into()),
            code_name:     None,
        }
    }

    /// The application (`xl` for Excel).
    #[must_use]
    pub fn app_name(&self) -> Option<&str> {
        self.app_name.as_deref()
    }

    pub fn set_app_name(&mut self, value: Option<&str>) -> &mut Self {
        self.app_name = value.map(str::to_string);
        self
    }

    /// The version of the application that edited the file last.
    #[must_use]
    pub fn last_edited(&self) -> Option<&str> {
        self.last_edited.as_deref()
    }

    pub fn set_last_edited(&mut self, value: Option<&str>) -> &mut Self {
        self.last_edited = value.map(str::to_string);
        self
    }

    /// The lowest version of the application that edited the file.
    #[must_use]
    pub fn lowest_edited(&self) -> Option<&str> {
        self.lowest_edited.as_deref()
    }

    pub fn set_lowest_edited(&mut self, value: Option<&str>) -> &mut Self {
        self.lowest_edited = value.map(str::to_string);
        self
    }

    /// The build of the application that saved the file.
    #[must_use]
    pub fn rup_build(&self) -> Option<&str> {
        self.rup_build.as_deref()
    }

    pub fn set_rup_build(&mut self, value: Option<&str>) -> &mut Self {
        self.rup_build = value.map(str::to_string);
        self
    }

    /// A GUID of the workbook for the application (`codeName`).
    #[must_use]
    pub fn code_name(&self) -> Option<&str> {
        self.code_name.as_deref()
    }

    pub fn set_code_name(&mut self, value: Option<&str>) -> &mut Self {
        self.code_name = value.map(str::to_string);
        self
    }

    pub(crate) fn read(e: &BytesStart<'_>) -> Self {
        let mut out = Self::default();
        for a in e.attributes().with_checks(false).flatten() {
            let value = Some(crate::reader::driver::unescaped_attribute_value(&a));
            match a.key.local_name().as_ref() {
                b"appName" => out.app_name = value,
                b"lastEdited" => out.last_edited = value,
                b"lowestEdited" => out.lowest_edited = value,
                b"rupBuild" => out.rup_build = value,
                b"codeName" => out.code_name = value,
                _ => {}
            }
        }
        out
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        for (name, value) in [
            ("appName", &self.app_name),
            ("lastEdited", &self.last_edited),
            ("lowestEdited", &self.lowest_edited),
            ("rupBuild", &self.rup_build),
            ("codeName", &self.code_name),
        ] {
            if let Some(v) = value {
                attributes.push((name, v.as_str()).into());
            }
        }
        write_start_tag(writer, "fileVersion", attributes, true);
    }
}

/// The sharing of the file (`fileSharing`): the password to modify it and
/// whether Excel recommends opening it read-only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileSharing {
    read_only_recommended: bool,
    /// The user who set the password.
    user_name:             Option<String>,
    /// The password as Excel 2007 hashes it (16 bits, hexadecimal).
    reservation_password:  Option<String>,
    algorithm_name:        Option<String>,
    hash_value:            Option<String>,
    salt_value:            Option<String>,
    spin_count:            Option<u32>,
}

impl FileSharing {
    /// Whether Excel recommends opening the file read-only.
    #[must_use]
    pub fn read_only_recommended(&self) -> bool {
        self.read_only_recommended
    }

    pub fn set_read_only_recommended(&mut self, value: bool) -> &mut Self {
        self.read_only_recommended = value;
        self
    }

    /// The user who set the password to modify.
    #[must_use]
    pub fn user_name(&self) -> Option<&str> {
        self.user_name.as_deref()
    }

    pub fn set_user_name(&mut self, value: Option<&str>) -> &mut Self {
        self.user_name = value.map(str::to_string);
        self
    }

    /// Whether the file has a password to modify (Excel asks for it when it
    /// opens the file, or opens it read-only).
    #[must_use]
    pub fn has_password(&self) -> bool {
        self.hash_value.is_some() || self.reservation_password.is_some()
    }

    /// Sets the password to modify the file (hashed with SHA-512, as Excel
    /// 2013 and later do); an empty one removes it.
    pub fn set_password(&mut self, password: &str) -> &mut Self {
        self.remove_password();
        if !password.is_empty() {
            let (algorithm, hash, salt, spin) = crate::helper::crypt::hash_password(password);
            self.algorithm_name = Some(algorithm);
            self.hash_value = Some(hash);
            self.salt_value = Some(salt);
            self.spin_count = Some(spin);
        }
        self
    }

    /// Removes the password to modify the file.
    pub fn remove_password(&mut self) -> &mut Self {
        self.reservation_password = None;
        self.algorithm_name = None;
        self.hash_value = None;
        self.salt_value = None;
        self.spin_count = None;
        self
    }

    /// Whether `password` is the password to modify the file (of those
    /// hashed with SHA-512, or as Excel 2007 does).
    #[must_use]
    pub fn check_password(&self, password: &str) -> bool {
        if let (Some(algorithm), Some(hash), Some(salt), Some(spin)) =
            (&self.algorithm_name, &self.hash_value, &self.salt_value, self.spin_count)
        {
            return crate::helper::crypt::check_password(password, algorithm, hash, salt, spin);
        }
        if let Some(legacy) = &self.reservation_password {
            return u16::from_str_radix(legacy, 16).is_ok_and(|v| v == crate::helper::crypt::legacy_password_hash(password));
        }
        false
    }

    /// Whether there is anything to write.
    pub(crate) fn is_empty(&self) -> bool {
        !self.read_only_recommended && self.user_name.is_none() && !self.has_password()
    }

    pub(crate) fn read(e: &BytesStart<'_>) -> Self {
        let mut out = Self::default();
        for a in e.attributes().with_checks(false).flatten() {
            let value = crate::reader::driver::unescaped_attribute_value(&a);
            match a.key.local_name().as_ref() {
                b"readOnlyRecommended" => out.read_only_recommended = value == "1" || value == "true",
                b"userName" => out.user_name = Some(value),
                b"reservationPassword" => out.reservation_password = Some(value),
                b"algorithmName" => out.algorithm_name = Some(value),
                b"hashValue" => out.hash_value = Some(value),
                b"saltValue" => out.salt_value = Some(value),
                b"spinCount" => out.spin_count = value.parse().ok(),
                _ => {}
            }
        }
        out
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        if self.is_empty() {
            return;
        }
        let spin = self.spin_count.map(|s| s.to_string());
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        if self.read_only_recommended {
            attributes.push(("readOnlyRecommended", "1").into());
        }
        for (name, value) in [
            ("userName", &self.user_name),
            ("reservationPassword", &self.reservation_password),
            ("algorithmName", &self.algorithm_name),
            ("hashValue", &self.hash_value),
            ("saltValue", &self.salt_value),
            ("spinCount", &spin),
        ] {
            if let Some(v) = value {
                attributes.push((name, v.as_str()).into());
            }
        }
        write_start_tag(writer, "fileSharing", attributes, true);
    }
}
