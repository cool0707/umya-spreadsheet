//! The (decompressed) `VBA/dir` stream: project information, references and
//! module records (\[MS-OVBA\] 2.3.4.2).

use super::{
    codepage,
    error::{
        VbaError,
        format_err,
    },
};

/// PROJECTSYSKIND value for 32-bit Windows.
pub(crate) const SYSKIND_WIN32: u32 = 1;
/// The only LCID \[MS-OVBA\] allows on write.
pub(crate) const LCID_EN_US: u32 = 0x0409;

/// MODULETYPE id of a procedural (standard) module.
const ID_TYPE_PROCEDURAL: u16 = 0x0021;
/// MODULETYPE id of a document, class or designer module.
const ID_TYPE_OBJECT: u16 = 0x0022;

/// One piece of the PROJECTREFERENCES record, in stream order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReferencePart {
    /// REFERENCENAME (0x0016, optional unicode half 0x003E).
    Name {
        name:    Vec<u8>,
        unicode: Option<Vec<u8>>,
    },
    /// REFERENCEORIGINAL (0x0033).
    Original { libid: Vec<u8> },
    /// REFERENCECONTROL (0x002F ... 0x0030 ...).
    Control {
        twiddled:          Vec<u8>,
        reserved1:         u32,
        reserved2:         u16,
        name_extended:     Option<(Vec<u8>, Option<Vec<u8>>)>,
        extended:          Vec<u8>,
        reserved4:         u32,
        reserved5:         u16,
        original_type_lib: [u8; 16],
        cookie:            u32,
    },
    /// REFERENCEREGISTERED (0x000D).
    Registered {
        libid:     Vec<u8>,
        reserved1: u32,
        reserved2: u16,
    },
    /// REFERENCEPROJECT (0x000E).
    Project {
        absolute: Vec<u8>,
        relative: Vec<u8>,
        major:    u32,
        minor:    u16,
    },
}

/// A MODULE record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DirModule {
    pub name:             String,
    pub stream_name:      String,
    /// Stream name decoded from the MBCS field (may differ from
    /// `stream_name`, which prefers the Unicode field).
    pub stream_name_mbcs: String,
    pub docstring:        String,
    pub offset:           u32,
    pub help_context:     u32,
    /// `true` for MODULETYPE 0x0021 (procedural module).
    pub procedural:       bool,
    pub readonly:         bool,
    pub private:          bool,
}

/// Parsed `dir` stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DirStream {
    pub syskind:        u32,
    pub compat_version: Option<u32>,
    pub lcid:           u32,
    pub lcid_invoke:    u32,
    pub codepage:       u16,
    pub name:           String,
    pub docstring:      String,
    pub help_file:      String,
    pub help_context:   u32,
    pub lib_flags:      u32,
    pub version_major:  u32,
    pub version_minor:  u16,
    /// `None` when the (optional) PROJECTCONSTANTS record is absent.
    pub constants:      Option<String>,
    /// The PROJECTREFERENCES bytes exactly as stored.
    pub references_raw: Vec<u8>,
    pub references:     Vec<ReferencePart>,
    pub modules:        Vec<DirModule>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos:  usize,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn peek_u16(&self) -> Option<u16> {
        let b = self.data.get(self.pos..self.pos + 2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u16(&mut self) -> Result<u16, VbaError> {
        match self.peek_u16() {
            Some(v) => {
                self.pos += 2;
                Ok(v)
            }
            None => format_err("dir stream is truncated"),
        }
    }

    fn u32(&mut self) -> Result<u32, VbaError> {
        match self.data.get(self.pos..self.pos + 4) {
            Some(b) => {
                self.pos += 4;
                Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            }
            None => format_err("dir stream is truncated"),
        }
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], VbaError> {
        if len > self.remaining() {
            return format_err("dir stream is truncated (record size exceeds the stream)");
        }
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    /// Reads a `size` field followed by that many bytes.
    fn sized(&mut self) -> Result<&'a [u8], VbaError> {
        let len = self.u32()? as usize;
        self.bytes(len)
    }
}

fn u32_of(data: &[u8], what: &str) -> Result<u32, VbaError> {
    match data {
        [a, b, c, d, ..] => Ok(u32::from_le_bytes([*a, *b, *c, *d])),
        _ => format_err(format!("dir stream: {what} record is too short")),
    }
}

fn u16_of(data: &[u8], what: &str) -> Result<u16, VbaError> {
    match data {
        [a, b, ..] => Ok(u16::from_le_bytes([*a, *b])),
        _ => format_err(format!("dir stream: {what} record is too short")),
    }
}

impl DirStream {
    /// Parses the decompressed `dir` stream.
    ///
    /// The parser follows the record order of the specification but tolerates
    /// the optional records being absent and unknown record ids inside a
    /// MODULE record (they are skipped).
    pub(crate) fn parse(data: &[u8]) -> Result<DirStream, VbaError> {
        let mut r = Reader { data, pos: 0 };

        let mut syskind = None;
        let mut compat_version = None;
        let mut lcid = LCID_EN_US;
        let mut lcid_invoke = LCID_EN_US;
        let mut cp: Option<u16> = None;
        let mut name_raw: Vec<u8> = Vec::new();
        let mut docstring_raw: Vec<u8> = Vec::new();
        let mut docstring_unicode: Option<Vec<u8>> = None;
        let mut help_file_raw: Vec<u8> = Vec::new();
        let mut help_context = 0u32;
        let mut lib_flags = 0u32;
        let mut version_major = 0u32;
        let mut version_minor = 0u16;
        let mut constants_raw: Option<Vec<u8>> = None;
        let mut constants_unicode: Option<Vec<u8>> = None;

        // PROJECTINFORMATION: everything up to the first reference record or
        // the PROJECTMODULES record.
        loop {
            let Some(id) = r.peek_u16() else {
                return format_err("dir stream ends before the PROJECTMODULES record");
            };
            match id {
                0x0016 | 0x0033 | 0x002F | 0x000D | 0x000E | 0x000F => break,
                _ => {}
            }
            r.pos += 2;
            if id == 0x0009 {
                // PROJECTVERSION: the "size" field is a reserved 0x00000004
                // but six bytes follow.
                let _reserved = r.u32()?;
                version_major = r.u32()?;
                version_minor = r.u16()?;
                continue;
            }
            let body = r.sized()?;
            match id {
                0x0001 => syskind = Some(u32_of(body, "PROJECTSYSKIND")?),
                0x004A => compat_version = Some(u32_of(body, "PROJECTCOMPATVERSION")?),
                0x0002 => lcid = u32_of(body, "PROJECTLCID")?,
                0x0014 => lcid_invoke = u32_of(body, "PROJECTLCIDINVOKE")?,
                0x0003 => cp = Some(u16_of(body, "PROJECTCODEPAGE")?),
                0x0004 => name_raw = body.to_vec(),
                0x0005 => docstring_raw = body.to_vec(),
                0x0040 => docstring_unicode = Some(body.to_vec()),
                0x0006 => help_file_raw = body.to_vec(),
                0x003D => {}
                0x0007 => help_context = u32_of(body, "PROJECTHELPCONTEXT")?,
                0x0008 => lib_flags = u32_of(body, "PROJECTLIBFLAGS")?,
                0x000C => constants_raw = Some(body.to_vec()),
                0x003C => constants_unicode = Some(body.to_vec()),
                other => {
                    return format_err(format!(
                        "dir stream: unexpected record 0x{other:04X} in the project information"
                    ));
                }
            }
        }
        let Some(syskind) = syskind else {
            return format_err("dir stream does not start with a PROJECTSYSKIND record");
        };
        let Some(codepage) = cp else {
            return format_err("dir stream has no PROJECTCODEPAGE record");
        };

        // PROJECTREFERENCES
        let references_start = r.pos;
        let references = parse_references(&mut r)?;
        let references_raw = data[references_start..r.pos].to_vec();

        // PROJECTMODULES
        let id = r.u16()?;
        if id != 0x000F {
            return format_err(format!(
                "dir stream: expected PROJECTMODULES, found 0x{id:04X}"
            ));
        }
        let count = u16_of(r.sized()?, "PROJECTMODULES")? as usize;
        if r.peek_u16() == Some(0x0013) {
            r.pos += 2;
            let _cookie = r.sized()?;
        }
        let mut modules = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            modules.push(parse_module(&mut r, codepage)?);
        }
        // The stream ends with the terminator 0x0010 and four reserved bytes;
        // nothing after the last module is needed.

        let text = |raw: &[u8], unicode: Option<&Vec<u8>>| -> Result<String, VbaError> {
            match unicode {
                Some(u) if !u.is_empty() || raw.is_empty() => Ok(codepage::from_utf16le(u)),
                _ => codepage::decode(codepage, raw),
            }
        };
        Ok(DirStream {
            syskind,
            compat_version,
            lcid,
            lcid_invoke,
            codepage,
            name: codepage::decode(codepage, &name_raw)?,
            docstring: text(&docstring_raw, docstring_unicode.as_ref())?,
            help_file: codepage::decode(codepage, &help_file_raw)?,
            help_context,
            lib_flags,
            version_major,
            version_minor,
            constants: match constants_raw {
                Some(raw) => Some(text(&raw, constants_unicode.as_ref())?),
                None => None,
            },
            references_raw,
            references,
            modules,
        })
    }

    /// Serialises the stream (uncompressed) using `self.codepage` for every
    /// MBCS string.
    ///
    /// `references` supplies the PROJECTREFERENCES bytes (normally
    /// `self.references_raw`, or re-encoded parts after a code page change).
    pub(crate) fn serialize(&self, references: &[u8]) -> Result<Vec<u8>, VbaError> {
        let cp = self.codepage;
        let mut out = Vec::with_capacity(1024);
        let enc = |text: &str, what: &str| codepage::encode(cp, text, what);

        put_record(&mut out, 0x0001, &self.syskind.to_le_bytes());
        if let Some(compat) = self.compat_version {
            put_record(&mut out, 0x004A, &compat.to_le_bytes());
        }
        put_record(&mut out, 0x0002, &self.lcid.to_le_bytes());
        put_record(&mut out, 0x0014, &self.lcid_invoke.to_le_bytes());
        put_record(&mut out, 0x0003, &cp.to_le_bytes());

        let name = enc(&self.name, "project name")?;
        if name.is_empty() || name.len() > 128 || name.contains(&0) {
            return Err(VbaError::InvalidRequest(format!(
                "project name `{}` must be 1 to 128 bytes long in code page {cp}",
                self.name
            )));
        }
        put_record(&mut out, 0x0004, &name);

        let docstring = enc(&self.docstring, "project description")?;
        put_record(&mut out, 0x0005, &docstring);
        put_record(&mut out, 0x0040, &codepage::utf16le(&self.docstring));

        let help_file = enc(&self.help_file, "project help file")?;
        put_record(&mut out, 0x0006, &help_file);
        put_record(&mut out, 0x003D, &help_file);

        put_record(&mut out, 0x0007, &self.help_context.to_le_bytes());
        put_record(&mut out, 0x0008, &self.lib_flags.to_le_bytes());

        out.extend_from_slice(&0x0009u16.to_le_bytes());
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&self.version_major.to_le_bytes());
        out.extend_from_slice(&self.version_minor.to_le_bytes());

        if let Some(constants) = &self.constants {
            put_record(&mut out, 0x000C, &enc(constants, "project constants")?);
            put_record(&mut out, 0x003C, &codepage::utf16le(constants));
        }

        out.extend_from_slice(references);

        let Ok(count) = u16::try_from(self.modules.len()) else {
            return Err(VbaError::InvalidRequest("too many modules".into()));
        };
        put_record(&mut out, 0x000F, &count.to_le_bytes());
        // PROJECTCOOKIE: "MUST be 0xFFFF on write".
        put_record(&mut out, 0x0013, &0xFFFFu16.to_le_bytes());

        for module in &self.modules {
            let what = format!("name of module `{}`", module.name);
            put_record(&mut out, 0x0019, &enc(&module.name, &what)?);
            put_record(&mut out, 0x0047, &codepage::utf16le(&module.name));
            put_record(&mut out, 0x001A, &enc(&module.stream_name, &what)?);
            put_record(&mut out, 0x0032, &codepage::utf16le(&module.stream_name));
            let what = format!("description of module `{}`", module.name);
            put_record(&mut out, 0x001C, &enc(&module.docstring, &what)?);
            put_record(&mut out, 0x0048, &codepage::utf16le(&module.docstring));
            put_record(&mut out, 0x0031, &module.offset.to_le_bytes());
            put_record(&mut out, 0x001E, &module.help_context.to_le_bytes());
            // MODULECOOKIE: "MUST be 0xFFFF on write".
            put_record(&mut out, 0x002C, &0xFFFFu16.to_le_bytes());
            let type_id = if module.procedural {
                ID_TYPE_PROCEDURAL
            } else {
                ID_TYPE_OBJECT
            };
            put_record(&mut out, type_id, &[]);
            if module.readonly {
                put_record(&mut out, 0x0025, &[]);
            }
            if module.private {
                put_record(&mut out, 0x0028, &[]);
            }
            put_record(&mut out, 0x002B, &[]);
        }

        // Terminator + reserved.
        put_record(&mut out, 0x0010, &[]);
        Ok(out)
    }
}

/// The 32-bit size field of `body`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "nothing in a VBA project comes near 4 GiB"
)]
fn size_field(body: &[u8]) -> [u8; 4] {
    (body.len() as u32).to_le_bytes()
}

/// Appends `id`, the 32-bit length of `body` and `body`. A record without a
/// body is the id followed by a zero "reserved" field, which has the same
/// layout.
fn put_record(out: &mut Vec<u8>, id: u16, body: &[u8]) {
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&size_field(body));
    out.extend_from_slice(body);
}

fn parse_reference_name(r: &mut Reader<'_>) -> Result<(Vec<u8>, Option<Vec<u8>>), VbaError> {
    // The id 0x0016 has been consumed by the caller.
    let name = r.sized()?.to_vec();
    // The Unicode half is mandatory in the specification but missing in
    // files written by some Mac versions of Office.
    let unicode = if r.peek_u16() == Some(0x003E) {
        r.pos += 2;
        Some(r.sized()?.to_vec())
    } else {
        None
    };
    Ok((name, unicode))
}

fn parse_references(r: &mut Reader<'_>) -> Result<Vec<ReferencePart>, VbaError> {
    let mut parts = Vec::new();
    loop {
        let Some(id) = r.peek_u16() else {
            return format_err("dir stream ends inside the PROJECTREFERENCES record");
        };
        if id == 0x000F {
            return Ok(parts);
        }
        r.pos += 2;
        match id {
            0x0016 => {
                let (name, unicode) = parse_reference_name(r)?;
                parts.push(ReferencePart::Name { name, unicode });
            }
            0x0033 => {
                let libid = r.sized()?.to_vec();
                parts.push(ReferencePart::Original { libid });
            }
            0x002F => {
                let _size_twiddled = r.u32()?;
                let twiddled = r.sized()?.to_vec();
                let reserved1 = r.u32()?;
                let reserved2 = r.u16()?;
                let mut name_extended = None;
                let mut next = r.u16()?;
                if next == 0x0016 {
                    name_extended = Some(parse_reference_name(r)?);
                    next = r.u16()?;
                }
                if next != 0x0030 {
                    return format_err(format!(
                        "dir stream: REFERENCECONTROL is followed by 0x{next:04X} instead of 0x0030"
                    ));
                }
                let _size_extended = r.u32()?;
                let extended = r.sized()?.to_vec();
                let reserved4 = r.u32()?;
                let reserved5 = r.u16()?;
                let mut original_type_lib = [0u8; 16];
                original_type_lib.copy_from_slice(r.bytes(16)?);
                let cookie = r.u32()?;
                parts.push(ReferencePart::Control {
                    twiddled,
                    reserved1,
                    reserved2,
                    name_extended,
                    extended,
                    reserved4,
                    reserved5,
                    original_type_lib,
                    cookie,
                });
            }
            0x000D => {
                let _size = r.u32()?;
                let libid = r.sized()?.to_vec();
                let reserved1 = r.u32()?;
                let reserved2 = r.u16()?;
                parts.push(ReferencePart::Registered {
                    libid,
                    reserved1,
                    reserved2,
                });
            }
            0x000E => {
                let _size = r.u32()?;
                let absolute = r.sized()?.to_vec();
                let relative = r.sized()?.to_vec();
                let major = r.u32()?;
                let minor = r.u16()?;
                parts.push(ReferencePart::Project {
                    absolute,
                    relative,
                    major,
                    minor,
                });
            }
            other => {
                return format_err(format!(
                    "dir stream: unknown reference record 0x{other:04X}"
                ));
            }
        }
    }
}

fn parse_module(r: &mut Reader<'_>, cp: u16) -> Result<DirModule, VbaError> {
    let first = r.u16()?;
    if first != 0x0019 {
        return format_err(format!(
            "dir stream: expected a MODULENAME record, found 0x{first:04X}"
        ));
    }
    let name_raw = r.sized()?.to_vec();
    let mut name_unicode: Option<Vec<u8>> = None;
    let mut stream_raw: Option<Vec<u8>> = None;
    let mut stream_unicode: Option<Vec<u8>> = None;
    let mut doc_raw: Vec<u8> = Vec::new();
    let mut doc_unicode: Option<Vec<u8>> = None;
    let mut offset = 0u32;
    let mut help_context = 0u32;
    let mut procedural = None;
    let mut readonly = false;
    let mut private = false;
    loop {
        let id = r.u16()?;
        let body = r.sized()?;
        match id {
            0x0047 => name_unicode = Some(body.to_vec()),
            0x001A => stream_raw = Some(body.to_vec()),
            0x0032 => stream_unicode = Some(body.to_vec()),
            0x001C => doc_raw = body.to_vec(),
            0x0048 => doc_unicode = Some(body.to_vec()),
            0x0031 => offset = u32_of(body, "MODULEOFFSET")?,
            0x001E => help_context = u32_of(body, "MODULEHELPCONTEXT")?,
            0x002C => {}
            ID_TYPE_PROCEDURAL => procedural = Some(true),
            ID_TYPE_OBJECT => procedural = Some(false),
            0x0025 => readonly = true,
            0x0028 => private = true,
            0x002B => break,
            0x0019 | 0x0010 | 0x000F => {
                return format_err("dir stream: MODULE record is not terminated");
            }
            _ => {} // unknown sub-record: ignore
        }
    }

    let name = match &name_unicode {
        Some(u) if !u.is_empty() => codepage::from_utf16le(u),
        _ => codepage::decode(cp, &name_raw)?,
    };
    if name.is_empty() {
        return format_err("dir stream: MODULE record with an empty name");
    }
    let stream_name_mbcs = match &stream_raw {
        Some(raw) if !raw.is_empty() => codepage::decode(cp, raw)?,
        _ => name.clone(),
    };
    let stream_name = match &stream_unicode {
        Some(u) if !u.is_empty() => codepage::from_utf16le(u),
        _ => stream_name_mbcs.clone(),
    };
    let docstring = match &doc_unicode {
        Some(u) if !u.is_empty() || doc_raw.is_empty() => codepage::from_utf16le(u),
        _ => codepage::decode(cp, &doc_raw)?,
    };
    Ok(DirModule {
        name,
        stream_name,
        stream_name_mbcs,
        docstring,
        offset,
        help_context,
        procedural: procedural.unwrap_or(false),
        readonly,
        private,
    })
}

/// Serialises reference parts, re-encoding every MBCS string from
/// `from_cp` to `to_cp`. Used when the project code page changes.
pub(crate) fn serialize_references(
    parts: &[ReferencePart],
    from_cp: u16,
    to_cp: u16,
) -> Result<Vec<u8>, VbaError> {
    let conv = |bytes: &[u8]| -> Result<Vec<u8>, VbaError> {
        if from_cp == to_cp || bytes.is_ascii() {
            return Ok(bytes.to_vec());
        }
        let text = codepage::decode_strict(from_cp, bytes, "project reference")?;
        codepage::encode(to_cp, &text, "project reference")
    };
    let put_name =
        |out: &mut Vec<u8>, name: &[u8], unicode: &Option<Vec<u8>>| -> Result<(), VbaError> {
            put_record(out, 0x0016, &conv(name)?);
            if let Some(unicode) = unicode {
                put_record(out, 0x003E, unicode);
            }
            Ok(())
        };

    let mut out = Vec::new();
    for part in parts {
        match part {
            ReferencePart::Name { name, unicode } => put_name(&mut out, name, unicode)?,
            ReferencePart::Original { libid } => put_record(&mut out, 0x0033, &conv(libid)?),
            ReferencePart::Control {
                twiddled,
                reserved1,
                reserved2,
                name_extended,
                extended,
                reserved4,
                reserved5,
                original_type_lib,
                cookie,
            } => {
                let twiddled = conv(twiddled)?;
                let mut body = Vec::new();
                body.extend_from_slice(&size_field(&twiddled));
                body.extend_from_slice(&twiddled);
                body.extend_from_slice(&reserved1.to_le_bytes());
                body.extend_from_slice(&reserved2.to_le_bytes());
                put_record(&mut out, 0x002F, &body);
                if let Some((name, unicode)) = name_extended {
                    put_name(&mut out, name, unicode)?;
                }
                let extended = conv(extended)?;
                let mut body = Vec::new();
                body.extend_from_slice(&size_field(&extended));
                body.extend_from_slice(&extended);
                body.extend_from_slice(&reserved4.to_le_bytes());
                body.extend_from_slice(&reserved5.to_le_bytes());
                body.extend_from_slice(original_type_lib);
                body.extend_from_slice(&cookie.to_le_bytes());
                put_record(&mut out, 0x0030, &body);
            }
            ReferencePart::Registered {
                libid,
                reserved1,
                reserved2,
            } => {
                let libid = conv(libid)?;
                let mut body = Vec::new();
                body.extend_from_slice(&size_field(&libid));
                body.extend_from_slice(&libid);
                body.extend_from_slice(&reserved1.to_le_bytes());
                body.extend_from_slice(&reserved2.to_le_bytes());
                put_record(&mut out, 0x000D, &body);
            }
            ReferencePart::Project {
                absolute,
                relative,
                major,
                minor,
            } => {
                let absolute = conv(absolute)?;
                let relative = conv(relative)?;
                let mut body = Vec::new();
                body.extend_from_slice(&size_field(&absolute));
                body.extend_from_slice(&absolute);
                body.extend_from_slice(&size_field(&relative));
                body.extend_from_slice(&relative);
                body.extend_from_slice(&major.to_le_bytes());
                body.extend_from_slice(&minor.to_le_bytes());
                put_record(&mut out, 0x000E, &body);
            }
        }
    }
    Ok(out)
}

/// The two references every project created by Excel starts with.
pub(crate) fn default_references() -> Vec<ReferencePart> {
    let registered = |name: &str, libid: &str| {
        [
            ReferencePart::Name {
                name:    name.as_bytes().to_vec(),
                unicode: Some(codepage::utf16le(name)),
            },
            ReferencePart::Registered {
                libid:     libid.as_bytes().to_vec(),
                reserved1: 0,
                reserved2: 0,
            },
        ]
    };
    let mut parts = Vec::new();
    parts.extend(registered(
        "stdole",
        "*\\G{00020430-0000-0000-C000-000000000046}#2.0#0#C:\\Windows\\System32\\stdole2.tlb#OLE Automation",
    ));
    parts.extend(registered(
        "Office",
        "*\\G{2DF8D04C-5BFA-101B-BDE5-00AA0044DE52}#2.0#0#C:\\Program Files\\Common Files\\Microsoft Shared\\OFFICE16\\MSO.DLL#Microsoft Office 16.0 Object Library",
    ));
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DirStream {
        let references = default_references();
        let references_raw = serialize_references(&references, 932, 932).unwrap();
        DirStream {
            syskind: SYSKIND_WIN32,
            compat_version: None,
            lcid: LCID_EN_US,
            lcid_invoke: LCID_EN_US,
            codepage: 932,
            name: "VBAProject".into(),
            docstring: "説明".into(),
            help_file: String::new(),
            help_context: 0,
            lib_flags: 0,
            version_major: 0x6000_0000,
            version_minor: 7,
            constants: Some("DEBUG_MODE = 1".into()),
            references_raw,
            references,
            modules: vec![
                DirModule {
                    name:             "ThisWorkbook".into(),
                    stream_name:      "ThisWorkbook".into(),
                    stream_name_mbcs: "ThisWorkbook".into(),
                    docstring:        String::new(),
                    offset:           0,
                    help_context:     0,
                    procedural:       false,
                    readonly:         false,
                    private:          false,
                },
                DirModule {
                    name:             "標準モジュール".into(),
                    stream_name:      "標準モジュール".into(),
                    stream_name_mbcs: "標準モジュール".into(),
                    docstring:        "メモ".into(),
                    offset:           0,
                    help_context:     3,
                    procedural:       true,
                    readonly:         true,
                    private:          true,
                },
            ],
        }
    }

    #[test]
    fn serialize_then_parse_is_identity() {
        let dir = sample();
        let bytes = dir.serialize(&dir.references_raw).unwrap();
        assert_eq!(&bytes[bytes.len() - 6..], &[0x10, 0, 0, 0, 0, 0]);
        let parsed = DirStream::parse(&bytes).unwrap();
        assert_eq!(parsed, dir);
        // Serialising the parsed form gives the same bytes again.
        assert_eq!(parsed.serialize(&parsed.references_raw).unwrap(), bytes);
    }

    #[test]
    fn registered_reference_layout() {
        let raw = serialize_references(&default_references()[..2], 1252, 1252).unwrap();
        // REFERENCENAME
        assert_eq!(&raw[..6], &[0x16, 0, 6, 0, 0, 0]);
        assert_eq!(&raw[6..12], b"stdole");
        assert_eq!(&raw[12..18], &[0x3E, 0, 12, 0, 0, 0]);
        // REFERENCEREGISTERED: Size = 4 + len + 4 + 2, SizeOfLibid = len
        let libid_len = 94u32;
        assert_eq!(&raw[30..32], &[0x0D, 0]);
        assert_eq!(&raw[32..36], &(libid_len + 10).to_le_bytes());
        assert_eq!(&raw[36..40], &libid_len.to_le_bytes());
        assert_eq!(raw.len(), 40 + libid_len as usize + 6);
        assert!(raw[raw.len() - 6..].iter().all(|b| *b == 0));
    }

    #[test]
    fn truncated_or_garbage_input_is_an_error() {
        let dir = sample();
        let bytes = dir.serialize(&dir.references_raw).unwrap();
        for cut in [0usize, 1, 5, 9, 20, 60, bytes.len() / 2, bytes.len() - 7] {
            assert!(DirStream::parse(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        assert!(DirStream::parse(&[0xFF; 64]).is_err());
    }
}
