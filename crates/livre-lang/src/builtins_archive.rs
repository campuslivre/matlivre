//! Archives: `zip`/`unzip`, `tar`/`untar` (plain or gzip-compressed) and `gzip`/`gunzip`.
//!
//! Entry names inside an archive are the paths as given (relative to the root folder) with
//! `/` separators, folders expanded recursively in name order. Extraction returns the
//! written paths, joined to the output folder with the platform separator.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::builtins::{arg, none, one, Ret};
use crate::builtins_files::{glob, join_parts};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["zip", "unzip", "tar", "untar", "gzip", "gunzip"];

fn err(e: impl std::fmt::Display) -> LangError {
    LangError::new(e.to_string(), 0)
}

fn names(v: &Value) -> R<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text()).collect(),
        Value::Str(s) => Ok(s.data.iter().map(|x| x.clone().unwrap_or_default()).collect()),
        other => Ok(vec![other.text()?]),
    }
}

fn cell_row(items: Vec<String>) -> Value {
    Value::Cell(CellArray::row(items.iter().map(|s| Value::chars(s)).collect()))
}

fn slashes(s: &str) -> String {
    s.replace('\\', "/")
}

/// Files to archive as (entry name, path on disk), expanding wildcards and folders.
fn collect(files: &[String], root: &str) -> R<Vec<(String, PathBuf)>> {
    fn walk(entry: &str, path: &Path, out: &mut Vec<(String, PathBuf)>) {
        if path.is_dir() {
            let mut kids: Vec<_> = std::fs::read_dir(path).map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
            kids.sort();
            for k in kids {
                walk(&format!("{}/{k}", entry.trim_end_matches('/')), &path.join(&k), out);
            }
        } else {
            out.push((entry.to_string(), path.to_path_buf()));
        }
    }
    let mut out = Vec::new();
    for f in files {
        let on_disk = if root.is_empty() || Path::new(f).is_absolute() { f.clone() } else { join_parts(&[root.to_string(), f.clone()]) };
        let matches = glob(&on_disk);
        if matches.is_empty() {
            return rt(format!("File \"{f}\" does not exist."));
        }
        // Entry names keep the folder part of the name as written.
        let given_dir = slashes(f).rfind('/').map(|i| slashes(f)[..=i].to_string()).unwrap_or_default();
        let wild = f.contains('*') || f.contains('?');
        for m in matches {
            let entry = if wild { format!("{given_dir}{}", m.file_name().unwrap().to_string_lossy()) } else { slashes(f) };
            let entry = entry.trim_start_matches("./").trim_start_matches('/').to_string();
            walk(&entry, &m, &mut out);
        }
    }
    Ok(out)
}

/// Archive name with `ext` appended when it has no extension.
fn with_ext(name: &str, ext: &str) -> String {
    if Path::new(name).extension().is_some() { name.to_string() } else { format!("{name}{ext}") }
}

fn opt_text(a: &[Value], i: usize) -> R<String> {
    a.get(i).map(|v| v.text()).transpose().map(Option::unwrap_or_default)
}

fn zip(a: &[Value], nargout: usize) -> Ret {
    let file = with_ext(&arg(a, 0, "zip")?.text()?, ".zip");
    let entries = collect(&names(arg(a, 1, "zip")?)?, &opt_text(a, 2)?)?;
    let out = std::fs::File::create(&file).map_err(err)?;
    let mut w = zip::ZipWriter::new(out);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, path) in &entries {
        w.start_file(name.as_str(), opts).map_err(err)?;
        w.write_all(&std::fs::read(path).map_err(err)?).map_err(err)?;
    }
    w.finish().map_err(err)?;
    if nargout > 0 { one(cell_row(entries.into_iter().map(|e| e.0).collect())) } else { none() }
}

/// Writes `data` to `entry` below `outdir`, returning the path as MATLAB reports it.
fn extract_to(outdir: &str, entry: &str, data: &[u8]) -> R<String> {
    let rel = entry.trim_start_matches('/');
    if rel.split('/').any(|p| p == "..") {
        return rt(format!("Archive entry '{entry}' is outside the output folder."));
    }
    let path = join_parts(&[outdir.to_string(), rel.to_string()]);
    if let Some(parent) = Path::new(&path).parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    std::fs::write(&path, data).map_err(err)?;
    Ok(path)
}

fn find_archive(name: &str, ext: &str, func: &str) -> R<String> {
    if Path::new(name).is_file() {
        return Ok(name.to_string());
    }
    let with = format!("{name}{ext}");
    if Path::new(&with).is_file() {
        return Ok(with);
    }
    rt(format!("Function {func} was unable to find file ''{name}''."))
}

fn unzip(a: &[Value], nargout: usize) -> Ret {
    let file = find_archive(&arg(a, 0, "unzip")?.text()?, ".zip", "UNZIP")?;
    let outdir = opt_text(a, 1)?;
    let mut z = zip::ZipArchive::new(std::fs::File::open(&file).map_err(err)?).map_err(err)?;
    let mut written = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(err)?;
        let name = f.name().to_string();
        if f.is_dir() {
            std::fs::create_dir_all(join_parts(&[outdir.clone(), name])).map_err(err)?;
            continue;
        }
        let mut data = Vec::new();
        f.read_to_end(&mut data).map_err(err)?;
        written.push(extract_to(&outdir, &name, &data)?);
    }
    if nargout > 0 { one(cell_row(written)) } else { none() }
}

fn compressed_tar(name: &str) -> bool {
    let l = name.to_lowercase();
    l.ends_with(".tgz") || l.ends_with(".gz")
}

fn tar(a: &[Value], nargout: usize) -> Ret {
    let file = with_ext(&arg(a, 0, "tar")?.text()?, ".tar");
    let entries = collect(&names(arg(a, 1, "tar")?)?, &opt_text(a, 2)?)?;
    let out = std::fs::File::create(&file).map_err(err)?;
    let sink: Box<dyn Write> = if compressed_tar(&file) { Box::new(flate2::write::GzEncoder::new(out, flate2::Compression::default())) } else { Box::new(out) };
    let mut b = tar::Builder::new(sink);
    for (name, path) in &entries {
        b.append_path_with_name(path, name).map_err(err)?;
    }
    b.into_inner().map_err(err)?.flush().map_err(err)?;
    if nargout > 0 { one(cell_row(entries.into_iter().map(|e| e.0).collect())) } else { none() }
}

fn untar(a: &[Value], nargout: usize) -> Ret {
    let file = find_archive(&arg(a, 0, "untar")?.text()?, ".tar", "UNTAR")?;
    let outdir = opt_text(a, 1)?;
    let raw = std::fs::read(&file).map_err(err)?;
    let data = if raw.starts_with(&[0x1f, 0x8b]) {
        let mut d = Vec::new();
        flate2::read::MultiGzDecoder::new(&raw[..]).read_to_end(&mut d).map_err(err)?;
        d
    } else {
        raw
    };
    let mut ar = tar::Archive::new(&data[..]);
    let mut written = Vec::new();
    for e in ar.entries().map_err(err)? {
        let mut e = e.map_err(err)?;
        let name = slashes(&e.path().map_err(err)?.to_string_lossy());
        if e.header().entry_type().is_dir() {
            std::fs::create_dir_all(join_parts(&[outdir.clone(), name])).map_err(err)?;
            continue;
        }
        if !e.header().entry_type().is_file() {
            continue;
        }
        let mut buf = Vec::new();
        e.read_to_end(&mut buf).map_err(err)?;
        written.push(extract_to(&outdir, &name, &buf)?);
    }
    if nargout > 0 { one(cell_row(written)) } else { none() }
}

/// The files named by `v`, wildcards expanded and folders recursed.
fn plain_files(v: &Value) -> R<Vec<PathBuf>> {
    Ok(collect(&names(v)?, "")?.into_iter().map(|(_, p)| p).collect())
}

/// Output path for `src` with a new file name, in `outdir` or next to the source.
fn output_path(src: &Path, name: &str, outdir: &str) -> String {
    let dir = if outdir.is_empty() { src.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default() } else { outdir.to_string() };
    if !dir.is_empty() {
        let _ = std::fs::create_dir_all(&dir);
    }
    join_parts(&[dir, name.to_string()])
}

fn gzip(a: &[Value], nargout: usize) -> Ret {
    let outdir = opt_text(a, 1)?;
    let mut written = Vec::new();
    for p in plain_files(arg(a, 0, "gzip")?)? {
        let name = format!("{}.gz", p.file_name().unwrap().to_string_lossy());
        let target = output_path(&p, &name, &outdir);
        let mut enc = flate2::write::GzEncoder::new(std::fs::File::create(&target).map_err(err)?, flate2::Compression::default());
        enc.write_all(&std::fs::read(&p).map_err(err)?).map_err(err)?;
        enc.finish().map_err(err)?;
        written.push(target);
    }
    if nargout > 0 { one(cell_row(written)) } else { none() }
}

fn gunzip(a: &[Value], nargout: usize) -> Ret {
    let outdir = opt_text(a, 1)?;
    let mut written = Vec::new();
    for p in plain_files(arg(a, 0, "gunzip")?)? {
        let file = p.file_name().unwrap().to_string_lossy().into_owned();
        let lower = file.to_lowercase();
        let name = if lower.ends_with(".tgz") {
            format!("{}.tar", &file[..file.len() - 4])
        } else if lower.ends_with(".gz") {
            file[..file.len() - 3].to_string()
        } else {
            continue;
        };
        let mut data = Vec::new();
        flate2::read::MultiGzDecoder::new(std::fs::File::open(&p).map_err(err)?).read_to_end(&mut data).map_err(err)?;
        let target = output_path(&p, &name, &outdir);
        std::fs::write(&target, data).map_err(err)?;
        written.push(target);
    }
    if nargout > 0 { one(cell_row(written)) } else { none() }
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "zip" => zip(a, nargout),
        "unzip" => unzip(a, nargout),
        "tar" => tar(a, nargout),
        "untar" => untar(a, nargout),
        "gzip" => gzip(a, nargout),
        "gunzip" => gunzip(a, nargout),
        _ => Ok(None),
    }
}
