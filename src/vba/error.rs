//! The error type of the [`vba`](super) module.

use std::fmt;

/// Everything that can go wrong while reading or writing a VBA project.
#[derive(Debug)]
#[non_exhaustive]
pub enum VbaError {
    /// The compound file (`vbaProject.bin`) could not be read or written.
    Cfb(std::io::Error),
    /// The data does not follow \[MS-OVBA\].
    Format(String),
    /// A module listed in the `dir` stream has no module stream.
    MissingModuleStream {
        /// Name of the module.
        module: String,
        /// Name of the stream the `dir` stream gives for it.
        stream: String,
    },
    /// The VBA project is password protected / locked for viewing (`DPB` /
    /// `CMG` in the `PROJECT` stream). Nothing is written.
    Protected,
    /// A document module was given for a code name that is not among the
    /// documents of the workbook.
    UnknownCodeName(String),
    /// The requested operation is outside the supported scope (creating a
    /// user form from its code alone, ...).
    UnsupportedFormat(String),
    /// The project that was just written did not read back as expected.
    Verify(String),
    /// Text cannot be represented in the project code page, or the code page
    /// itself is not supported.
    Encoding(String),
    /// The request itself is inconsistent (bad module name, removal of a
    /// document module, ...).
    InvalidRequest(String),
}

impl fmt::Display for VbaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VbaError::Cfb(e) => write!(f, "compound file error: {e}"),
            VbaError::Format(m) => write!(f, "invalid VBA project data: {m}"),
            VbaError::MissingModuleStream { module, stream } => write!(
                f,
                "invalid VBA project data: module `{module}` has no stream `{stream}` in the VBA \
                 storage"
            ),
            VbaError::Protected => {
                write!(
                    f,
                    "the VBA project is password protected (locked for viewing)"
                )
            }
            VbaError::UnknownCodeName(n) => {
                write!(
                    f,
                    "the workbook has no sheet or workbook with code name `{n}`"
                )
            }
            VbaError::UnsupportedFormat(m) => write!(f, "unsupported: {m}"),
            VbaError::Verify(m) => write!(f, "verification after writing failed: {m}"),
            VbaError::Encoding(m) => write!(f, "code page error: {m}"),
            VbaError::InvalidRequest(m) => write!(f, "invalid request: {m}"),
        }
    }
}

impl std::error::Error for VbaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VbaError::Cfb(e) => Some(e),
            _ => None,
        }
    }
}

pub(crate) fn format_err<T>(msg: impl Into<String>) -> Result<T, VbaError> {
    Err(VbaError::Format(msg.into()))
}
