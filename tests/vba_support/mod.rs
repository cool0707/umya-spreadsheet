//! Helpers shared by the tests of the `vba` module (`vba_*.rs`).
#![allow(dead_code)]

use std::{
    fs,
    io::{
        Cursor,
        Read,
    },
    path::{
        Path,
        PathBuf,
    },
};

use umya_spreadsheet::vba::{
    ModuleKind,
    VbaModule,
};

/// Path of a fixture in `tests/test_files/vba` (see the `README.md` there).
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("test_files")
        .join("vba")
        .join(name)
}

pub fn fixture_bytes(name: &str) -> Vec<u8> {
    fs::read(fixture(name)).expect("fixture exists")
}

pub fn module(kind: ModuleKind, name: &str, source: &str) -> VbaModule {
    VbaModule {
        name: name.to_string(),
        kind,
        source: source.to_string(),
    }
}

pub fn standard(name: &str, source: &str) -> VbaModule {
    module(ModuleKind::Standard, name, source)
}

pub fn class(name: &str, source: &str) -> VbaModule {
    module(ModuleKind::Class, name, source)
}

/// Decompressed contents of a zip entry.
pub fn zip_entry(path: &Path, name: &str) -> Option<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(fs::read(path).unwrap())).unwrap();
    let mut file = archive.by_name(name).ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
    Some(data)
}

/// `(module name, source)` of every module, read with the independent
/// `ovba` crate.
pub fn ovba_modules(bin: &[u8]) -> Vec<(String, String)> {
    let project = ovba::open_project(bin.to_vec()).expect("ovba opens the project");
    project
        .modules
        .iter()
        .map(|m| {
            let source = project
                .module_source(&m.name)
                .expect("ovba reads the module source");
            (m.name.clone(), source)
        })
        .collect()
}

/// All streams of a compound file as `(path, contents)`, sorted by path.
pub fn cfb_streams(bin: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut file = cfb::CompoundFile::open(Cursor::new(bin.to_vec())).expect("valid compound file");
    let entries: Vec<cfb::Entry> = file.walk().filter(cfb::Entry::is_stream).collect();
    let mut result = Vec::new();
    for entry in entries {
        let mut data = Vec::new();
        file.open_stream(entry.path())
            .unwrap()
            .read_to_end(&mut data)
            .unwrap();
        result.push((entry.path().to_string_lossy().replace('\\', "/"), data));
    }
    result.sort();
    result
}

/// All storages of a compound file as `(path, clsid text)`, sorted by path.
pub fn cfb_storages(bin: &[u8]) -> Vec<(String, String)> {
    let file = cfb::CompoundFile::open(Cursor::new(bin.to_vec())).expect("valid compound file");
    let mut result: Vec<(String, String)> = file
        .walk()
        .filter(cfb::Entry::is_storage)
        .map(|e| {
            (
                e.path().to_string_lossy().replace('\\', "/"),
                e.clsid().to_string(),
            )
        })
        .collect();
    result.sort();
    result
}

pub fn cfb_stream(bin: &[u8], path: &str) -> Option<Vec<u8>> {
    cfb_streams(bin)
        .into_iter()
        .find(|(p, _)| p == path)
        .map(|(_, d)| d)
}
