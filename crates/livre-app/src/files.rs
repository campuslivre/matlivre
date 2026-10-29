//! The Files panel's model: a tree rooted at "This PC" (the drives), with folders listed on
//! demand and re-listed every couple of seconds while they are open.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::icons::{self, Ic};
use crate::os;

const RELIST: Duration = Duration::from_secs(2);

pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub dir: bool,
}

pub struct Row {
    /// `None` for "This PC".
    pub path: Option<PathBuf>,
    pub name: String,
    pub depth: usize,
    pub icon: Ic,
    pub dir: bool,
    pub open: bool,
}

pub struct FileTree {
    pc_open: bool,
    open: HashSet<PathBuf>,
    drives: Vec<(PathBuf, String)>,
    listings: HashMap<PathBuf, (Instant, Vec<Entry>)>,
}

fn hidden(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM
        meta.file_attributes() & 0x6 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        false
    }
}

fn list_dir(dir: &Path) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Entry> = rd
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let name = e.file_name().to_string_lossy().into_owned();
            if hidden(&meta) || name.starts_with('.') {
                return None;
            }
            Some(Entry { path: e.path(), name, dir: meta.is_dir() })
        })
        .collect();
    out.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

impl FileTree {
    pub fn new() -> Self {
        Self { pc_open: true, open: HashSet::new(), drives: os::drives(), listings: HashMap::new() }
    }

    /// Opens every folder from the drive down to `dir`.
    pub fn reveal(&mut self, dir: &Path) {
        self.pc_open = true;
        for a in dir.ancestors() {
            self.open.insert(a.to_path_buf());
        }
    }

    pub fn toggle(&mut self, path: Option<&Path>) {
        match path {
            None => self.pc_open = !self.pc_open,
            Some(p) => {
                if !self.open.remove(p) {
                    self.open.insert(p.to_path_buf());
                    self.listings.remove(p);
                }
            }
        }
    }

    fn children(&mut self, dir: &Path) -> &[Entry] {
        let stale = self.listings.get(dir).is_none_or(|(t, _)| t.elapsed() > RELIST);
        if stale {
            self.listings.insert(dir.to_path_buf(), (Instant::now(), list_dir(dir)));
        }
        &self.listings[dir].1
    }

    /// The visible rows, depth first.
    pub fn rows(&mut self) -> Vec<Row> {
        let mut rows = vec![Row { path: None, name: "This PC".into(), depth: 0, icon: Ic::Pc, dir: true, open: self.pc_open }];
        if !self.pc_open {
            return rows;
        }
        let drives = self.drives.clone();
        for (root, label) in drives {
            let open = self.open.contains(&root);
            rows.push(Row { path: Some(root.clone()), name: label, depth: 1, icon: Ic::Drive, dir: true, open });
            if open {
                self.walk(&root, 2, &mut rows);
            }
        }
        rows
    }

    fn walk(&mut self, dir: &Path, depth: usize, rows: &mut Vec<Row>) {
        let entries: Vec<(PathBuf, String, bool)> = self.children(dir).iter().map(|e| (e.path.clone(), e.name.clone(), e.dir)).collect();
        for (path, name, is_dir) in entries {
            let open = is_dir && self.open.contains(&path);
            let icon = if !is_dir {
                icons::for_file(&name)
            } else if open {
                Ic::FolderOpen
            } else {
                Ic::Folder
            };
            rows.push(Row { path: Some(path.clone()), name, depth, icon, dir: is_dir, open });
            if open {
                self.walk(&path, depth + 1, rows);
            }
        }
    }
}
