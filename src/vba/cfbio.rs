//! Reading and writing whole compound files (\[MS-CFB\]) through the `cfb`
//! crate.
//!
//! A project is always rebuilt into a fresh, compact compound file instead
//! of being patched in place, so no remains of deleted streams (old p-code,
//! old source) survive in free sectors. Storages keep their CLSID, state
//! bits and timestamps; stream contents are copied byte for byte.

use std::{
    collections::{
        HashMap,
        HashSet,
    },
    fmt,
    io::{
        Cursor,
        Read,
        Write,
    },
    path::{
        Component,
        PathBuf,
    },
};

use super::error::VbaError;

/// A storage or stream below the root storage.
#[derive(Clone)]
pub(crate) struct Node {
    /// Names from the root down to this object.
    pub path:  Vec<String>,
    /// Stream contents; `None` for a storage.
    pub data:  Option<Vec<u8>>,
    /// Directory entry this node was read from (CLSID, state bits).
    pub meta:  Option<cfb::Entry>,
    /// Creation and modification time (FILETIME) of a storage.
    pub times: (u64, u64),
    /// CLSID (wire byte order) of a new storage that has no `meta`.
    pub clsid: Option<[u8; 16]>,
}

impl Node {
    pub(crate) fn stream(path: Vec<String>, data: Vec<u8>) -> Node {
        Node {
            path,
            data: Some(data),
            meta: None,
            times: (0, 0),
            clsid: None,
        }
    }

    pub(crate) fn storage(path: Vec<String>) -> Node {
        Node {
            path,
            data: None,
            meta: None,
            times: (0, 0),
            clsid: None,
        }
    }

    pub(crate) fn is_storage(&self) -> bool {
        self.data.is_none()
    }

    pub(crate) fn name(&self) -> &str {
        self.path.last().map_or("", String::as_str)
    }
}

/// All objects of a compound file, parents before children.
#[derive(Clone, Default)]
pub(crate) struct Tree {
    pub root_meta:  Option<cfb::Entry>,
    /// CLSID (wire byte order) of the root storage of a new file without
    /// `root_meta`.
    pub root_clsid: Option<[u8; 16]>,
    pub root_times: (u64, u64),
    pub nodes:      Vec<Node>,
}

// (`cfb::Entry` has no `Debug`.)
impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Node")
            .field("path", &self.path)
            .field("len", &self.data.as_ref().map(Vec::len))
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for Tree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tree")
            .field("nodes", &self.nodes)
            .finish_non_exhaustive()
    }
}

fn cfb_err(e: std::io::Error) -> VbaError {
    VbaError::Cfb(e)
}

fn path_of(chain: &[String]) -> Result<PathBuf, VbaError> {
    let mut path = PathBuf::from("/");
    for name in chain {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains(['/', '\\', ':', '!'])
            || name.encode_utf16().count() > 31
        {
            return Err(VbaError::Format(format!(
                "`{name}` is not a valid compound file stream or storage name"
            )));
        }
        path.push(name);
    }
    Ok(path)
}

/// Reads every storage and stream of a compound file.
pub(crate) fn read_tree(bytes: &[u8]) -> Result<Tree, VbaError> {
    read_tree_at(bytes, &[])
}

/// Reads every storage and stream below the storage `storage` (its names
/// from the root down; empty for the root storage), as a tree of its own:
/// the paths start below `storage`, and `storage` takes the place of the
/// root. Nothing outside `storage` is read.
pub(crate) fn read_tree_at(bytes: &[u8], storage: &[String]) -> Result<Tree, VbaError> {
    let mut file = cfb::CompoundFile::open(Cursor::new(bytes)).map_err(cfb_err)?;
    let entries: Vec<cfb::Entry> = file.walk().collect();
    let raw_times = raw_directory(bytes)
        .map(|d| storage_times(&d))
        .unwrap_or_default();
    // Names in a compound file compare case-insensitively.
    let inside = |chain: &[String]| {
        chain.len() >= storage.len()
            && chain
                .iter()
                .zip(storage)
                .all(|(a, b)| a.to_uppercase() == b.to_uppercase())
    };

    let mut tree = Tree::default();
    let mut found = false;
    for entry in entries {
        if entry.is_root() {
            if storage.is_empty() {
                found = true;
                tree.root_times = raw_times.get(&Vec::new()).copied().unwrap_or((0, 0));
                tree.root_meta = Some(entry);
            }
            continue;
        }
        let mut chain = Vec::new();
        for component in entry.path().components() {
            if let Component::Normal(name) = component {
                match name.to_str() {
                    Some(name) => chain.push(name.to_string()),
                    None => {
                        return Err(VbaError::Format(
                            "compound file entry name is not valid Unicode".into(),
                        ));
                    }
                }
            }
        }
        if chain.is_empty() || !inside(&chain) {
            continue;
        }
        if chain.len() == storage.len() {
            if entry.is_storage() && !found {
                found = true;
                tree.root_times = raw_times.get(&chain).copied().unwrap_or((0, 0));
                tree.root_meta = Some(entry);
            }
            continue;
        }
        let times = raw_times.get(&chain).copied().unwrap_or((0, 0));
        let chain = chain[storage.len()..].to_vec();
        if entry.is_stream() {
            // (A stream cannot be longer than the file that holds it.)
            let capacity = usize::try_from(entry.len()).map_or(0, |n| n.min(bytes.len()));
            let mut data = Vec::with_capacity(capacity);
            file.open_stream(entry.path())
                .map_err(cfb_err)?
                .read_to_end(&mut data)
                .map_err(cfb_err)?;
            tree.nodes.push(Node {
                path:  chain,
                data:  Some(data),
                meta:  Some(entry),
                times: (0, 0),
                clsid: None,
            });
        } else {
            tree.nodes.push(Node {
                path: chain,
                data: None,
                meta: Some(entry),
                times,
                clsid: None,
            });
        }
    }
    if !found {
        return Err(VbaError::Format(format!(
            "the compound file has no storage `{}`",
            storage.join("/")
        )));
    }
    Ok(tree)
}

/// Ordering of sibling names in a compound file directory: shorter names
/// first, then by upper-cased text.
fn name_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16()
        .count()
        .cmp(&b.encode_utf16().count())
        .then_with(|| a.to_uppercase().cmp(&b.to_uppercase()))
}

/// Order in which the sorted `items` have to be inserted into a plain
/// binary search tree so that the tree comes out balanced.
fn balanced_order<T: Copy>(items: &[T], out: &mut Vec<T>) {
    if items.is_empty() {
        return;
    }
    let mid = items.len() / 2;
    out.push(items[mid]);
    balanced_order(&items[..mid], out);
    balanced_order(&items[mid + 1..], out);
}

/// Writes `tree` as a new version 3 (512-byte sector) compound file, the
/// flavour Office itself uses for `vbaProject.bin`.
pub(crate) fn write_tree(tree: &Tree) -> Result<Vec<u8>, VbaError> {
    let mut seen: HashSet<Vec<String>> = HashSet::new();
    for node in &tree.nodes {
        let key: Vec<String> = node.path.iter().map(|n| n.to_uppercase()).collect();
        if !seen.insert(key) {
            return Err(VbaError::Format(format!(
                "duplicate compound file entry `{}`",
                node.path.join("/")
            )));
        }
    }

    let mut file =
        cfb::CompoundFile::create_with_version(cfb::Version::V3, Cursor::new(Vec::new()))
            .map_err(cfb_err)?;
    if let Some(meta) = &tree.root_meta {
        file.set_storage_clsid("/", *meta.clsid())
            .map_err(cfb_err)?;
        file.set_state_bits("/", meta.state_bits())
            .map_err(cfb_err)?;
    } else if let Some(clsid) = tree.root_clsid {
        file.set_storage_clsid("/", uuid::Uuid::from_bytes_le(clsid))
            .map_err(cfb_err)?;
    }

    // Children of every storage, keyed by the parent's path.
    let mut children: HashMap<&[String], Vec<usize>> = HashMap::new();
    for (index, node) in tree.nodes.iter().enumerate() {
        let Some((_, parent)) = node.path.split_last() else {
            return Err(VbaError::Format(
                "compound file entry without a name".into(),
            ));
        };
        children.entry(parent).or_default().push(index);
    }

    let mut pending: Vec<Vec<String>> = vec![Vec::new()];
    let mut written = 0usize;
    while let Some(parent) = pending.pop() {
        let Some(indices) = children.get(parent.as_slice()) else {
            continue;
        };
        let mut sorted = indices.clone();
        sorted.sort_by(|a, b| name_order(tree.nodes[*a].name(), tree.nodes[*b].name()));
        let mut order = Vec::with_capacity(sorted.len());
        balanced_order(&sorted, &mut order);
        for index in order {
            let node = &tree.nodes[index];
            let path = path_of(&node.path)?;
            if let Some(data) = &node.data {
                let mut stream = file.create_new_stream(&path).map_err(cfb_err)?;
                stream.write_all(data).map_err(cfb_err)?;
            } else {
                file.create_storage(&path).map_err(cfb_err)?;
                if let Some(meta) = &node.meta {
                    file.set_storage_clsid(&path, *meta.clsid())
                        .map_err(cfb_err)?;
                } else if let Some(clsid) = node.clsid.filter(|c| *c != [0; 16]) {
                    file.set_storage_clsid(&path, uuid::Uuid::from_bytes_le(clsid))
                        .map_err(cfb_err)?;
                }
                pending.push(node.path.clone());
            }
            if let Some(meta) = &node.meta {
                if meta.state_bits() != 0 {
                    file.set_state_bits(&path, meta.state_bits())
                        .map_err(cfb_err)?;
                }
            }
            written += 1;
        }
    }
    if written != tree.nodes.len() {
        return Err(VbaError::Format(
            "compound file entry whose parent storage does not exist".into(),
        ));
    }

    file.flush().map_err(cfb_err)?;
    let mut bytes = file.into_inner().into_inner();

    // The `cfb` crate stamps new storages with the current time. Restore
    // the original times (zero, "not recorded", for new storages) so that
    // the output does not depend on when it was produced.
    let mut wanted: HashMap<Vec<String>, (u64, u64)> = HashMap::new();
    wanted.insert(Vec::new(), (0, tree.root_times.1));
    for node in tree.nodes.iter().filter(|n| n.is_storage()) {
        wanted.insert(node.path.clone(), node.times);
    }
    if let Some(directory) = raw_directory(&bytes) {
        for (path, entry) in entry_paths(&directory) {
            if entry.object_type != 1 && entry.object_type != 5 {
                continue;
            }
            let (created, modified) = wanted.get(&path).copied().unwrap_or((0, 0));
            bytes[entry.offset + 100..entry.offset + 108].copy_from_slice(&created.to_le_bytes());
            bytes[entry.offset + 108..entry.offset + 116].copy_from_slice(&modified.to_le_bytes());
        }
    }
    Ok(bytes)
}

/// A directory entry as stored in the file (\[MS-CFB\] 2.6.1).
#[derive(Clone, Debug)]
struct RawEntry {
    /// Offset of the 128-byte entry within the file.
    offset:      usize,
    name:        String,
    /// 1 = storage, 2 = stream, 5 = root storage.
    object_type: u8,
    left:        u32,
    right:       u32,
    child:       u32,
    created:     u64,
    modified:    u64,
}

const NO_STREAM: u32 = 0xFFFF_FFFF;
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const MAX_REGULAR_SECTOR: u32 = 0xFFFF_FFFA;

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let b = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn le_u64(bytes: &[u8], at: usize) -> Option<u64> {
    let b = bytes.get(at..at + 8)?;
    let mut raw = [0u8; 8];
    raw.copy_from_slice(b);
    Some(u64::from_le_bytes(raw))
}

/// Reads the directory entries straight from the file. Returns `None` when
/// the structure is not understood (the caller then simply does without the
/// raw timestamps).
fn raw_directory(bytes: &[u8]) -> Option<Vec<RawEntry>> {
    if bytes.get(..8)? != [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1] {
        return None;
    }
    let sector_shift = le_u16(bytes, 30)?;
    if sector_shift != 9 && sector_shift != 12 {
        return None;
    }
    let sector_size = 1usize << sector_shift;
    let sector_offset = |sector: u32| -> Option<usize> {
        if sector > MAX_REGULAR_SECTOR {
            return None;
        }
        let offset = (sector as usize).checked_add(1)?.checked_mul(sector_size)?;
        if offset.checked_add(sector_size)? > bytes.len() {
            return None;
        }
        Some(offset)
    };
    let max_sectors = bytes.len() / sector_size + 1;

    // DIFAT: 109 entries in the header, then a chain of DIFAT sectors.
    let mut fat_sectors: Vec<u32> = Vec::new();
    for i in 0..109 {
        let sector = le_u32(bytes, 76 + 4 * i)?;
        if sector <= MAX_REGULAR_SECTOR {
            fat_sectors.push(sector);
        }
    }
    let mut difat_sector = le_u32(bytes, 68)?;
    let mut guard = 0usize;
    while difat_sector <= MAX_REGULAR_SECTOR {
        guard += 1;
        if guard > max_sectors {
            return None;
        }
        let offset = sector_offset(difat_sector)?;
        let per_sector = sector_size / 4 - 1;
        for i in 0..per_sector {
            let sector = le_u32(bytes, offset + 4 * i)?;
            if sector <= MAX_REGULAR_SECTOR {
                fat_sectors.push(sector);
            }
        }
        difat_sector = le_u32(bytes, offset + 4 * per_sector)?;
    }
    let fat_entry = |sector: u32| -> Option<u32> {
        let per_sector = sector_size / 4;
        let fat_sector = *fat_sectors.get(sector as usize / per_sector)?;
        le_u32(
            bytes,
            sector_offset(fat_sector)? + 4 * (sector as usize % per_sector),
        )
    };

    let mut entries = Vec::new();
    let mut sector = le_u32(bytes, 48)?;
    let mut guard = 0usize;
    while sector != END_OF_CHAIN {
        guard += 1;
        if guard > max_sectors {
            return None;
        }
        let offset = sector_offset(sector)?;
        for i in 0..sector_size / 128 {
            let at = offset + 128 * i;
            let name_len = (le_u16(bytes, at + 64)? as usize).min(64);
            let units: Vec<u16> = (0..name_len.saturating_sub(2) / 2)
                .filter_map(|k| le_u16(bytes, at + 2 * k))
                .collect();
            entries.push(RawEntry {
                offset:      at,
                name:        String::from_utf16_lossy(&units),
                object_type: *bytes.get(at + 66)?,
                left:        le_u32(bytes, at + 68)?,
                right:       le_u32(bytes, at + 72)?,
                child:       le_u32(bytes, at + 76)?,
                created:     le_u64(bytes, at + 100)?,
                modified:    le_u64(bytes, at + 108)?,
            });
        }
        sector = fat_entry(sector)?;
    }
    if entries.first()?.object_type != 5 {
        return None;
    }
    Some(entries)
}

/// Resolves the path (names below the root) of every reachable entry.
fn entry_paths(directory: &[RawEntry]) -> Vec<(Vec<String>, &RawEntry)> {
    let mut result = Vec::new();
    let Some(root) = directory.first() else {
        return result;
    };
    result.push((Vec::new(), root));
    let mut visited = vec![false; directory.len()];
    visited[0] = true;
    // (entry id, path of the storage that contains it)
    let mut stack: Vec<(u32, Vec<String>)> = vec![(root.child, Vec::new())];
    while let Some((id, parent)) = stack.pop() {
        if id == NO_STREAM {
            continue;
        }
        let Some(entry) = directory.get(id as usize) else {
            continue;
        };
        if std::mem::replace(&mut visited[id as usize], true) {
            continue;
        }
        let mut path = parent.clone();
        path.push(entry.name.clone());
        stack.push((entry.left, parent.clone()));
        stack.push((entry.right, parent));
        if entry.object_type == 1 {
            stack.push((entry.child, path.clone()));
        }
        result.push((path, entry));
    }
    result
}

fn storage_times(directory: &[RawEntry]) -> HashMap<Vec<String>, (u64, u64)> {
    entry_paths(directory)
        .into_iter()
        .filter(|(_, e)| e.object_type == 1 || e.object_type == 5)
        .map(|(path, e)| (path, (e.created, e.modified)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> Tree {
        let p = |parts: &[&str]| parts.iter().map(ToString::to_string).collect::<Vec<_>>();
        let mut nodes = vec![
            Node::stream(p(&["PROJECT"]), b"ID=1\r\n".to_vec()),
            Node::storage(p(&["VBA"])),
            Node::stream(p(&["VBA", "dir"]), vec![1, 2, 3]),
            Node::stream(
                p(&["VBA", "Big"]),
                (0..10_000u32).map(|i| (i % 251) as u8).collect(),
            ),
            Node::stream(p(&["VBA", "Empty"]), Vec::new()),
            Node::storage(p(&["Form1"])),
            Node::stream(p(&["Form1", "\u{1}CompObj"]), vec![9; 97]),
            Node::stream(p(&["Form1", "\u{3}VBFrame"]), b"VERSION 5.00\r\n".to_vec()),
            Node::storage(p(&["Form1", "i01"])),
            Node::stream(p(&["Form1", "i01", "f"]), vec![7; 40]),
        ];
        for i in 0..40 {
            nodes.push(Node::stream(
                p(&["VBA", &format!("Module{i}")]),
                vec![u8::try_from(i).unwrap(); 100 + i],
            ));
        }
        nodes[5].times = (131_715_775_748_160_000, 131_715_775_748_190_000);
        Tree {
            root_meta: None,
            root_clsid: None,
            root_times: (0, 130_595_336_503_470_000),
            nodes,
        }
    }

    #[test]
    fn write_then_read_preserves_everything() {
        let tree = sample_tree();
        let bytes = write_tree(&tree).unwrap();
        assert_eq!(bytes.len() % 512, 0);
        let back = read_tree(&bytes).unwrap();
        assert_eq!(back.nodes.len(), tree.nodes.len());
        for node in &tree.nodes {
            let found = back
                .nodes
                .iter()
                .find(|n| n.path == node.path)
                .expect("node exists");
            assert_eq!(found.data, node.data, "{:?}", node.path);
            assert_eq!(found.times, node.times, "{:?}", node.path);
        }
        assert_eq!(back.root_times, (0, 130_595_336_503_470_000));
        // Deterministic output.
        assert_eq!(write_tree(&tree).unwrap(), bytes);
        // And stable under a second round trip.
        assert_eq!(write_tree(&back).unwrap(), bytes);
    }

    #[test]
    fn invalid_names_and_orphans_are_rejected() {
        let mut tree = sample_tree();
        tree.nodes.push(Node::stream(vec!["a/b".into()], vec![]));
        assert!(write_tree(&tree).is_err());
        let mut tree = sample_tree();
        tree.nodes
            .push(Node::stream(vec!["Missing".into(), "x".into()], vec![]));
        assert!(write_tree(&tree).is_err());
        let mut tree = sample_tree();
        tree.nodes
            .push(Node::stream(vec!["project".into()], vec![]));
        assert!(write_tree(&tree).is_err());
    }

    #[test]
    fn a_storage_is_read_as_a_tree_of_its_own() {
        let mut tree = sample_tree();
        for node in &mut tree.nodes {
            node.path.insert(0, "Outer".into());
        }
        let mut outer = Node::storage(vec!["Outer".into()]);
        outer.times = (131_715_775_748_160_000, 131_715_775_748_170_000);
        tree.nodes.insert(0, outer);
        tree.nodes
            .push(Node::stream(vec!["Workbook".into()], vec![1, 2, 3]));
        let bytes = write_tree(&tree).unwrap();

        // Names compare case-insensitively.
        let inner = read_tree_at(&bytes, &["OUTER".to_string()]).unwrap();
        assert_eq!(
            inner.root_times,
            (131_715_775_748_160_000, 131_715_775_748_170_000)
        );
        let expected = sample_tree();
        assert_eq!(inner.nodes.len(), expected.nodes.len());
        for node in &expected.nodes {
            let found = inner
                .nodes
                .iter()
                .find(|n| n.path == node.path)
                .expect("node exists");
            assert_eq!(found.data, node.data, "{:?}", node.path);
            assert_eq!(found.times, node.times, "{:?}", node.path);
        }
        // What lies outside the storage is not read.
        assert!(inner.nodes.iter().all(|n| n.path != ["Workbook"]));

        assert!(read_tree_at(&bytes, &["Missing".to_string()]).is_err());
        // A stream is not a storage.
        assert!(read_tree_at(&bytes, &["Workbook".to_string()]).is_err());
        assert_eq!(
            read_tree_at(&bytes, &[]).unwrap().nodes.len(),
            tree.nodes.len()
        );
    }

    #[test]
    fn garbage_is_not_a_compound_file() {
        assert!(read_tree(b"PK\x03\x04 not a compound file").is_err());
        assert!(read_tree(&[]).is_err());
        assert!(raw_directory(&[0u8; 600]).is_none());
    }

    #[test]
    fn balanced_insertion_order() {
        let mut out = Vec::new();
        balanced_order(&[1, 2, 3, 4, 5, 6, 7], &mut out);
        assert_eq!(out, vec![4, 2, 1, 3, 6, 5, 7]);
    }
}
