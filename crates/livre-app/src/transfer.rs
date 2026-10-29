//! Moving a tab between windows. The tab is written to a temporary live script plus a small
//! manifest (original path, unsaved flag, untitled name); the manifest's path is handed to
//! another running instance over a loopback socket, or to a new instance on its command line.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use eframe::egui;
use livre_doc::Document;

const PREFIX: &str = "matlivre-tab-";
const MAGIC: &str = "matlivre-tab 1";

pub struct Payload {
    pub doc: Document,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub name: String,
}

/// Writes the tab to the temp folder and returns the manifest path.
pub fn write(doc: &Document, path: Option<&Path>, dirty: bool, name: &str) -> Result<PathBuf, String> {
    static N: AtomicU32 = AtomicU32::new(0);
    let stem = format!("{PREFIX}{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed));
    let dir = std::env::temp_dir();
    let doc_file = dir.join(format!("{stem}.mlx"));
    let manifest = dir.join(format!("{stem}.txt"));
    doc.save(&doc_file).map_err(|e| e.to_string())?;
    let path = path.map(|p| p.display().to_string()).unwrap_or_default();
    let text = format!("{MAGIC}\n{}\n{path}\n{}\n{name}\n", doc_file.display(), u8::from(dirty));
    std::fs::write(&manifest, text).map_err(|e| e.to_string())?;
    Ok(manifest)
}

/// Reads a manifest written by [`write`] and deletes both temporary files.
pub fn read(manifest: &Path) -> Result<Payload, String> {
    let text = std::fs::read_to_string(manifest).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 5 || lines[0] != MAGIC {
        return Err("not a matlivre tab".into());
    }
    let doc_file = PathBuf::from(lines[1]);
    let doc = Document::load(&doc_file).map_err(|e| e.to_string());
    remove_temp(&doc_file);
    remove_temp(manifest);
    Ok(Payload {
        doc: doc?,
        path: (!lines[2].is_empty()).then(|| PathBuf::from(lines[2])),
        dirty: lines[3] == "1",
        name: lines[4].to_string(),
    })
}

/// Deletes a file only if it is one of ours: the socket accepts paths from any local process.
fn remove_temp(p: &Path) {
    let ours = p.parent() == Some(std::env::temp_dir().as_path())
        && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(PREFIX));
    if ours {
        let _ = std::fs::remove_file(p);
    }
}

/// Starts accepting tabs from other instances. Returns the port and the manifests received.
pub fn listen(ctx: egui::Context) -> Option<(u16, Receiver<PathBuf>)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() {
                continue;
            }
            let manifest = PathBuf::from(line.trim_end());
            if !manifest.is_file() {
                continue;
            }
            let _ = (&stream).write_all(b"ok\n");
            if tx.send(manifest).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });
    Some((port, rx))
}

/// Hands a manifest to the instance listening on `port`. False if it did not take it.
pub fn send(port: u16, manifest: &Path) -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_secs(1)) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    if writeln!(s, "{}", manifest.display()).is_err() {
        return false;
    }
    let mut reply = String::new();
    BufReader::new(&s).read_line(&mut reply).is_ok() && reply.trim() == "ok"
}

/// Starts a new instance showing the tab, with its window's top-left corner at `at`
/// (logical points).
pub fn spawn_window(manifest: &Path, at: Option<egui::Pos2>) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--tab").arg(manifest);
    if let Some(p) = at {
        cmd.arg("--at").arg(format!("{},{}", p.x.round(), p.y.round()));
    }
    cmd.spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use livre_doc::BlockKind;

    #[test]
    fn tab_round_trips_between_instances() {
        let mut doc = Document::new_script();
        doc.push(BlockKind::Heading, "Results");
        doc.push(BlockKind::Code, "x = 1:3;\ndisp(x)");
        let manifest = write(&doc, None, true, "untitled2.mlx").unwrap();

        let (tx, rx) = mpsc::channel();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let stream = listener.incoming().next().unwrap().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            (&stream).write_all(b"ok\n").unwrap();
            tx.send(PathBuf::from(line.trim_end())).unwrap();
        });
        assert!(send(port, &manifest));

        let got = read(&rx.recv().unwrap()).unwrap();
        assert!(got.dirty && got.path.is_none() && got.name == "untitled2.mlx");
        let texts: Vec<_> = got.doc.blocks.iter().map(|b| (b.kind, b.text.as_str())).collect();
        let want: Vec<_> = doc.blocks.iter().map(|b| (b.kind, b.text.as_str())).collect();
        assert_eq!(texts, want);
        // Both temporary files are gone.
        assert!(!manifest.exists());
    }

    #[test]
    fn only_our_temp_files_are_deleted() {
        let keep = std::env::temp_dir().join("not-matlivre-keep.txt");
        std::fs::write(&keep, "x").unwrap();
        remove_temp(&keep);
        assert!(keep.exists());
        std::fs::remove_file(keep).unwrap();
    }
}
