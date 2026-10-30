//! Files and folders: names (`fullfile`, `fileparts`, ...), operations (`dir`, `mkdir`,
//! `copyfile`, `delete`, ...), low-level I/O on file identifiers (`fopen`, `fgetl`, `fread`,
//! `fwrite`, `fscanf`, `fseek`, ...), whole-file text (`fileread`, `readlines`, `writelines`),
//! the environment (`getenv`, `setenv`, `computer`) and `system`.
//!
//! Open files live in a table keyed by MATLAB file identifiers (3 and up; 1 and 2 are the
//! output and error streams). A file is read into memory when opened for reading; writes go
//! straight to disk, and files opened for update are written back when closed.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::builtins::{arg, none, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "fullfile", "fileparts", "filesep", "pathsep", "tempdir", "tempname", "matlabroot", "userpath", "filemarker", "dir", "ls", "isfile", "isfolder",
    "isdir", "mkdir", "rmdir", "delete", "copyfile", "movefile", "fopen", "fclose", "fgetl", "fgets", "feof", "ferror", "frewind", "fseek",
    "ftell", "fread", "fwrite", "fscanf", "fileread", "readlines", "writelines", "getenv", "setenv", "unsetenv", "isenv", "computer", "system",
    "dos", "unix", "cmdsep",
];

struct OpenFile {
    path: PathBuf,
    mode: String,
    /// Contents (read and update modes) or nothing (write-only modes).
    data: Vec<u8>,
    pos: usize,
    /// Text mode ('t'): \n is written as \r\n and \r\n read as \n.
    text: bool,
    dirty: bool,
    last_error: String,
}

impl OpenFile {
    fn readable(&self) -> bool {
        self.mode.contains('r') || self.mode.contains('+')
    }

    fn append(&self) -> bool {
        self.mode.contains('a')
    }

    fn write(&mut self, bytes: &[u8]) -> R<()> {
        let bytes: Vec<u8> = if self.text { bytes.iter().flat_map(|&b| if b == b'\n' { vec![b'\r', b'\n'] } else { vec![b] }).collect() } else { bytes.to_vec() };
        if self.readable() {
            // Update modes keep the file in memory and write it back on close.
            if self.append() {
                self.pos = self.data.len();
            }
            let end = (self.pos + bytes.len()).min(self.data.len());
            self.data.splice(self.pos..end, bytes.iter().copied());
            self.pos += bytes.len();
            self.dirty = true;
            return Ok(());
        }
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&self.path).map_err(|e| LangError::new(e.to_string(), 0))?;
        f.write_all(&bytes).map_err(|e| LangError::new(e.to_string(), 0))?;
        self.pos += bytes.len();
        Ok(())
    }

    fn flush(&mut self) {
        if self.dirty {
            let _ = std::fs::write(&self.path, &self.data);
            self.dirty = false;
        }
    }
}

thread_local! {
    static FILES: RefCell<BTreeMap<i32, OpenFile>> = const { RefCell::new(BTreeMap::new()) };
}

fn with_file<T>(fid: i32, f: impl FnOnce(&mut OpenFile) -> R<T>) -> R<T> {
    FILES.with(|t| match t.borrow_mut().get_mut(&fid) {
        Some(file) => f(file),
        None => rt("Invalid file identifier. Use fopen to generate a valid file identifier."),
    })
}

fn fid_arg(a: &[Value], name: &str) -> R<i32> {
    Ok(arg(a, 0, name)?.scalar()? as i32)
}

/// Writes text to a file identifier: 1 (output) and 2 (error) go to the output, others to
/// the open file. Used by fprintf and fwrite.
pub fn write_text(it: &mut Interpreter, fid: i32, s: &str) -> R<()> {
    match fid {
        1 => {
            it.emit(OutputKind::Text(s.to_string()));
            Ok(())
        }
        2 => {
            it.emit(OutputKind::Text(s.to_string()));
            Ok(())
        }
        _ => with_file(fid, |f| f.write(s.as_bytes())),
    }
}

/// Closes every open file (writing back files opened for update), as at the end of a run.
pub fn close_all() {
    FILES.with(|t| {
        for (_, mut f) in std::mem::take(&mut *t.borrow_mut()) {
            f.flush();
        }
    });
}

fn text_out(s: &str, as_string: bool) -> Value {
    if as_string { Value::string(s) } else if s.is_empty() { Value::Num(Matrix::new(0, 0, Vec::new()).cast_to(Kind::Char)) } else { Value::chars(s) }
}

fn text_in(v: &Value) -> R<(String, bool)> {
    match v {
        Value::Str(s) if s.data.len() == 1 => Ok((s.data[0].clone().unwrap_or_default(), true)),
        other => Ok((other.text()?, false)),
    }
}

const SEP: char = if cfg!(windows) { '\\' } else { '/' };

fn is_sep(c: char) -> bool {
    c == '/' || (cfg!(windows) && c == '\\')
}

/// fullfile: parts joined with one separator, separators normalised.
pub(crate) fn join_parts(parts: &[String]) -> String {
    let mut out = String::new();
    for p in parts.iter().filter(|p| !p.is_empty()) {
        if out.is_empty() {
            out.push_str(p);
        } else {
            let trimmed_out = out.trim_end_matches(is_sep).to_string();
            let trimmed_p = p.trim_start_matches(is_sep);
            out = format!("{trimmed_out}{SEP}{trimmed_p}");
        }
    }
    // Repeated separators collapse (a UNC prefix keeps its two).
    let mut norm = String::new();
    let mut prev_sep = false;
    for (i, c) in out.chars().enumerate() {
        if is_sep(c) {
            if prev_sep && i > 1 {
                continue;
            }
            norm.push(if cfg!(windows) { SEP } else { c });
            prev_sep = true;
        } else {
            norm.push(c);
            prev_sep = false;
        }
    }
    norm
}

fn fileparts(path: &str) -> (String, String, String) {
    let cut = path.rfind(is_sep);
    let (dir, file) = match cut {
        Some(0) => (path[..1].to_string(), &path[1..]),
        Some(i) => (path[..i].to_string(), &path[i + 1..]),
        None => (String::new(), path),
    };
    match file.rfind('.') {
        Some(i) => (dir, file[..i].to_string(), file[i..].to_string()),
        None => (dir, file.to_string(), String::new()),
    }
}

/// Files matching a name with * wildcards in the last component.
pub(crate) fn glob(pattern: &str) -> Vec<PathBuf> {
    let p = Path::new(pattern);
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !name.contains('*') && !name.contains('?') {
        return if p.exists() { vec![p.to_path_buf()] } else { Vec::new() };
    }
    let dir = p.parent().filter(|d| !d.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|r| r.flatten().filter(|e| wildcard(&name, &e.file_name().to_string_lossy())).map(|e| e.path()).collect())
        .unwrap_or_default();
    out.sort();
    out
}

fn wildcard(pat: &str, s: &str) -> bool {
    fn go(p: &[char], s: &[char]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], s) || (!s.is_empty() && go(p, &s[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &s[1..]),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => go(&p[1..], &s[1..]),
            _ => false,
        }
    }
    go(&pat.chars().collect::<Vec<_>>(), &s.chars().collect::<Vec<_>>())
}

fn modified_datenum(meta: &std::fs::Metadata) -> f64 {
    let ms = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as f64).unwrap_or(0.0);
    (ms + crate::time::local_offset_ms()) / 86_400_000.0 + 719_529.0
}

/// dir: a struct array (name, folder, date, bytes, isdir, datenum).
fn dir_listing(target: &str) -> R<StructArray> {
    let path = Path::new(target);
    let entries: Vec<(String, PathBuf)> = if path.is_dir() {
        let mut v: Vec<(String, PathBuf)> = vec![(".".into(), path.to_path_buf()), ("..".into(), path.join(".."))];
        let mut rest: Vec<(String, PathBuf)> = std::fs::read_dir(path).map(|r| r.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).collect()).unwrap_or_default();
        rest.sort_by(|a, b| a.0.cmp(&b.0));
        v.extend(rest);
        v
    } else {
        glob(target).into_iter().map(|p| (p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), p)).collect()
    };
    let fields = ["name", "folder", "date", "bytes", "isdir", "datenum"].iter().map(|s| s.to_string()).collect();
    let mut s = StructArray::empty_with(fields);
    for (name, p) in entries {
        let meta = std::fs::metadata(&p).ok();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let folder = if name == "." || name == ".." { path.canonicalize().unwrap_or(path.to_path_buf()) } else { p.parent().map(|q| q.canonicalize().unwrap_or(q.to_path_buf())).unwrap_or_default() };
        let folder = folder.display().to_string().trim_start_matches(r"\\?\").to_string();
        let dn = meta.as_ref().map(modified_datenum).unwrap_or(0.0);
        let date = crate::time::format_datetime((dn - 719_529.0) * 86_400_000.0, "dd-MMM-yyyy HH:mm:ss");
        s.elems.push(vec![
            Value::chars(&name),
            Value::chars(&folder),
            Value::chars(&date),
            Value::num(if is_dir { 0.0 } else { meta.as_ref().map(|m| m.len() as f64).unwrap_or(0.0) }),
            Value::bool(is_dir),
            Value::num(dn),
        ]);
    }
    let n = s.elems.len();
    s.dims = if n == 0 { vec![0, 1] } else { vec![n, 1] };
    Ok(s)
}

fn io_err(e: std::io::Error) -> LangError {
    LangError::new(e.to_string(), 0)
}

/// fread/fwrite precision: (element bytes, reader kind, output kind).
fn precision(p: &str) -> R<(usize, Kind, Kind)> {
    let p = p.trim();
    let (src, out) = match p.split_once("=>") {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => match p.strip_prefix('*') {
            Some(a) => (a, Some(a)),
            None => (p, None),
        },
    };
    let kind_of = |s: &str| -> R<Kind> {
        Ok(match s {
            "uchar" | "uint8" | "unsigned char" => Kind::Int(IntClass::U8),
            "schar" | "int8" | "signed char" => Kind::Int(IntClass::I8),
            "char" | "char*1" => Kind::Char,
            "int16" | "short" => Kind::Int(IntClass::I16),
            "uint16" | "ushort" => Kind::Int(IntClass::U16),
            "int32" | "int" | "long" => Kind::Int(IntClass::I32),
            "uint32" | "uint" | "ulong" => Kind::Int(IntClass::U32),
            "int64" => Kind::Int(IntClass::I64),
            "uint64" => Kind::Int(IntClass::U64),
            "single" | "float32" | "float" | "real*4" => Kind::Single,
            "double" | "float64" | "real*8" => Kind::Double,
            "bit1" | "ubit1" => return rt("Bit precisions are not supported."),
            other => return rt(format!("Invalid precision '{other}'.")),
        })
    };
    let k = kind_of(src)?;
    let bytes = match k {
        Kind::Double => 8,
        Kind::Single => 4,
        Kind::Char | Kind::Logical => 1,
        Kind::Int(i) => i.bytes(),
    };
    let out_kind = match out {
        Some(o) => kind_of(o)?,
        None => Kind::Double,
    };
    Ok((bytes, k, out_kind))
}

fn decode(b: &[u8], k: Kind) -> f64 {
    match k {
        Kind::Double => f64::from_le_bytes(b.try_into().unwrap()),
        Kind::Single => f32::from_le_bytes(b.try_into().unwrap()) as f64,
        Kind::Char | Kind::Logical | Kind::Int(IntClass::U8) => b[0] as f64,
        Kind::Int(IntClass::I8) => b[0] as i8 as f64,
        Kind::Int(IntClass::I16) => i16::from_le_bytes(b.try_into().unwrap()) as f64,
        Kind::Int(IntClass::U16) => u16::from_le_bytes(b.try_into().unwrap()) as f64,
        Kind::Int(IntClass::I32) => i32::from_le_bytes(b.try_into().unwrap()) as f64,
        Kind::Int(IntClass::U32) => u32::from_le_bytes(b.try_into().unwrap()) as f64,
        Kind::Int(IntClass::I64) => i64::from_le_bytes(b.try_into().unwrap()) as f64,
        Kind::Int(IntClass::U64) => u64::from_le_bytes(b.try_into().unwrap()) as f64,
    }
}

fn encode(x: f64, k: Kind) -> Vec<u8> {
    // Values saturate to the type's range, as fwrite does.
    let sat = |lo: f64, hi: f64| x.round().clamp(lo, hi);
    match k {
        Kind::Double => x.to_le_bytes().to_vec(),
        Kind::Single => (x as f32).to_le_bytes().to_vec(),
        Kind::Char | Kind::Logical | Kind::Int(IntClass::U8) => vec![sat(0.0, 255.0) as u8],
        Kind::Int(IntClass::I8) => (sat(-128.0, 127.0) as i8).to_le_bytes().to_vec(),
        Kind::Int(IntClass::I16) => (sat(-32768.0, 32767.0) as i16).to_le_bytes().to_vec(),
        Kind::Int(IntClass::U16) => (sat(0.0, 65535.0) as u16).to_le_bytes().to_vec(),
        Kind::Int(IntClass::I32) => (sat(i32::MIN as f64, i32::MAX as f64) as i32).to_le_bytes().to_vec(),
        Kind::Int(IntClass::U32) => (sat(0.0, u32::MAX as f64) as u32).to_le_bytes().to_vec(),
        Kind::Int(IntClass::I64) => (x.round() as i64).to_le_bytes().to_vec(),
        Kind::Int(IntClass::U64) => (x.round().max(0.0) as u64).to_le_bytes().to_vec(),
    }
}

fn fread(a: &[Value], nargout: usize) -> Ret {
    let fid = fid_arg(a, "fread")?;
    // fread(fid), fread(fid, size), fread(fid, precision), fread(fid, size, precision).
    let (size, prec) = match (a.get(1), a.get(2)) {
        (Some(p), None) if p.is_text() => (None, p.text()?),
        (Some(s), p) => (Some(s.to_matrix()?), p.map(|v| v.text()).transpose()?.unwrap_or_else(|| "uint8".into())),
        (None, _) => (None, "uint8".into()),
    };
    let (w, src, out_kind) = precision(&prec)?;
    let bytes = with_file(fid, |f| {
        if !f.readable() {
            return rt("Invalid file identifier for reading.");
        }
        let avail = (f.data.len() - f.pos) / w;
        let want = match &size {
            Some(m) if m.numel() == 2 => if m.data[1].is_infinite() { avail } else { (m.data[0] * m.data[1]) as usize },
            Some(m) if m.numel() == 1 && m.data[0].is_finite() => m.data[0] as usize,
            _ => avail,
        };
        let n = want.min(avail);
        let b = f.data[f.pos..f.pos + n * w].to_vec();
        f.pos += n * w;
        Ok(b)
    })?;
    let vals: Vec<f64> = bytes.chunks(w).map(|c| decode(c, src)).collect();
    let n = vals.len();
    let m = match &size {
        Some(sz) if sz.numel() == 2 => {
            let r = sz.data[0] as usize;
            let c = n.div_ceil(r.max(1));
            let mut v = vals;
            v.resize(r * c, 0.0);
            Matrix::new(r, c, v)
        }
        _ => Matrix::new(n, 1, vals),
    };
    let m = m.cast_to(out_kind);
    let mut out = vec![Value::Num(m)];
    if nargout > 1 {
        out.push(Value::num(n as f64));
    }
    Ok(Some(out))
}

fn fwrite(it: &mut Interpreter, a: &[Value]) -> Ret {
    let fid = fid_arg(a, "fwrite")?;
    let m = arg(a, 1, "fwrite")?.to_matrix()?;
    let prec = a.get(2).map(|v| v.text()).transpose()?.unwrap_or_else(|| "uint8".into());
    let (_, k, _) = precision(&prec)?;
    let bytes: Vec<u8> = m.data.iter().flat_map(|&x| encode(x, k)).collect();
    if fid == 1 || fid == 2 {
        write_text(it, fid, &String::from_utf8_lossy(&bytes))?;
    } else {
        with_file(fid, |f| f.write(&bytes))?;
    }
    one(Value::num(m.numel() as f64))
}

/// The next line, with (`keep`) or without its line ending; None at end of file.
fn next_line(f: &mut OpenFile, keep: bool) -> Option<String> {
    if f.pos >= f.data.len() {
        return None;
    }
    let rest = &f.data[f.pos..];
    let end = rest.iter().position(|&b| b == b'\n').map(|p| p + 1).unwrap_or(rest.len());
    let raw = &rest[..end];
    f.pos += end;
    let text = String::from_utf8_lossy(raw).into_owned();
    Some(if keep { if f.text { text.replace("\r\n", "\n") } else { text } } else { text.trim_end_matches('\n').trim_end_matches('\r').to_string() })
}

fn read_text_file(path: &str) -> R<String> {
    let bytes = std::fs::read(path).map_err(|_| LangError::new(format!("Could not open file {path}. No such file or directory."), 0))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn fopen(a: &[Value], nargout: usize) -> Ret {
    let first = arg(a, 0, "fopen")?;
    // fopen('all') and fopen(fid).
    if first.is_text() && first.text()? == "all" {
        let ids: Vec<f64> = FILES.with(|t| t.borrow().keys().map(|&k| k as f64).collect());
        return one(if ids.is_empty() { Value::Num(Matrix::new(1, 0, Vec::new())) } else { Value::Num(Matrix::row(ids)) });
    }
    if !first.is_text() && !matches!(first, Value::Str(_)) {
        let fid = first.scalar()? as i32;
        let info = FILES.with(|t| t.borrow().get(&fid).map(|f| (f.path.display().to_string(), f.mode.clone())));
        let (name, mode) = match fid {
            0 => ("\"stdin\"".to_string(), "r".to_string()),
            1 => ("\"stdout\"".to_string(), "w".to_string()),
            2 => ("\"stderr\"".to_string(), "w".to_string()),
            _ => info.unwrap_or_default(),
        };
        return Ok(Some(vec![Value::chars(&name), Value::chars(&mode), Value::chars("ieee-le"), Value::chars("UTF-8")].into_iter().take(nargout.max(1)).collect()));
    }
    let path = first.text()?;
    let mode = a.get(1).map(|v| v.text()).transpose()?.unwrap_or_else(|| "r".into());
    let base: String = mode.chars().filter(|c| !matches!(c, 't' | 'b')).collect();
    let text = mode.contains('t');
    let opened: Result<Vec<u8>, std::io::Error> = match base.as_str() {
        "r" | "r+" => std::fs::read(&path),
        "w" | "w+" => std::fs::write(&path, b"").map(|_| Vec::new()),
        "a" => std::fs::OpenOptions::new().create(true).append(true).open(&path).map(|_| Vec::new()),
        "a+" => std::fs::OpenOptions::new().create(true).append(true).open(&path).and_then(|_| std::fs::read(&path)),
        "W" | "A" => std::fs::write(&path, b"").map(|_| Vec::new()),
        _ => return rt(format!("Invalid permission '{mode}'.")),
    };
    match opened {
        Ok(data) => {
            let data = if text { String::from_utf8_lossy(&data).replace("\r\n", "\n").into_bytes() } else { data };
            let fid = FILES.with(|t| {
                let mut t = t.borrow_mut();
                let id = (3..).find(|k| !t.contains_key(k)).unwrap();
                let pos = if base.starts_with('a') { data.len() } else { 0 };
                t.insert(id, OpenFile { path: PathBuf::from(&path), mode: base.clone(), data, pos, text, dirty: false, last_error: String::new() });
                id
            });
            let mut out = vec![Value::num(fid as f64)];
            if nargout > 1 {
                out.push(Value::Num(Matrix::new(0, 0, Vec::new()).cast_to(Kind::Char)));
            }
            Ok(Some(out))
        }
        Err(e) => {
            let msg = match e.kind() {
                std::io::ErrorKind::NotFound => "No such file or directory".to_string(),
                std::io::ErrorKind::PermissionDenied => "Permission denied".to_string(),
                _ => e.to_string(),
            };
            Ok(Some(vec![Value::num(-1.0), Value::chars(&msg)].into_iter().take(nargout.max(1)).collect()))
        }
    }
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "fullfile" => {
            // Cell or string array arguments give one path per element.
            let lists: Vec<(Vec<String>, Option<Dims>, bool)> = a
                .iter()
                .map(|v| match v {
                    Value::Cell(c) => Ok((c.data.iter().map(|x| x.text()).collect::<R<Vec<_>>>()?, Some(c.dims.clone()), false)),
                    Value::Str(s) if s.data.len() != 1 => Ok((s.data.iter().map(|x| x.clone().unwrap_or_default()).collect(), Some(s.dims.clone()), true)),
                    Value::Str(s) => Ok((vec![s.data[0].clone().unwrap_or_default()], None, true)),
                    other => Ok((vec![other.text()?], None, false)),
                })
                .collect::<R<_>>()?;
            let any_string = lists.iter().any(|l| l.2);
            match lists.iter().find_map(|l| l.1.clone()) {
                Some(dims) => {
                    let n: usize = dims.iter().product();
                    let paths: Vec<String> = (0..n).map(|i| join_parts(&lists.iter().map(|l| if l.0.len() == 1 { l.0[0].clone() } else { l.0[i].clone() }).collect::<Vec<_>>())).collect();
                    if any_string {
                        one(Value::Str(StrArray::from_dims(dims, paths.into_iter().map(Some).collect())))
                    } else {
                        one(Value::Cell(CellArray::from_dims(dims, paths.iter().map(|p| Value::chars(p)).collect())))
                    }
                }
                None => one(text_out(&join_parts(&lists.iter().map(|l| l.0[0].clone()).collect::<Vec<_>>()), any_string)),
            }
        }
        "fileparts" => {
            let (p, as_string) = text_in(arg(a, 0, name)?)?;
            let (d, n, e) = fileparts(&p);
            Ok(Some([d, n, e].iter().map(|s| text_out(s, as_string)).take(nargout.max(1)).collect()))
        }
        "filesep" => one(Value::chars(&SEP.to_string())),
        "pathsep" => one(Value::chars(if cfg!(windows) { ";" } else { ":" })),
        "cmdsep" => one(Value::chars(if cfg!(windows) { "&&" } else { ";" })),
        "filemarker" => one(Value::chars(">")),
        "tempdir" => {
            let mut t = std::env::temp_dir().display().to_string();
            if !t.ends_with(SEP) {
                t.push(SEP);
            }
            one(Value::chars(&t))
        }
        "tempname" => {
            let dir = match a.first() {
                Some(d) => PathBuf::from(d.text()?),
                None => std::env::temp_dir(),
            };
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            one(Value::chars(&dir.join(format!("tp{:x}_{:04x}", nanos, std::process::id() & 0xffff)).display().to_string()))
        }
        "matlabroot" => {
            let root = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
            one(Value::chars(&root.display().to_string()))
        }
        "userpath" => {
            let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
            one(Value::chars(&Path::new(&home).join("Documents").join("MATLAB").display().to_string()))
        }
        "dir" | "ls" => {
            let target = a.first().map(|v| v.text()).transpose()?.unwrap_or_else(|| ".".into());
            let s = dir_listing(&target)?;
            if name == "dir" && nargout > 0 {
                return one(Value::Struct(s));
            }
            let names: Vec<String> = s.elems.iter().map(|e| e[0].text().unwrap_or_default()).collect();
            if name == "ls" && nargout > 0 {
                let w = names.iter().map(|n| n.len()).max().unwrap_or(0);
                let rows: Vec<String> = names.iter().map(|n| format!("{n:<w$}")).collect();
                return one(char_rows(&rows));
            }
            let w = names.iter().map(|n| n.len()).max().unwrap_or(0) + 2;
            let row: String = names.iter().map(|n| format!("{n:<w$}")).collect();
            it.emit(OutputKind::Text(format!("\n{row}\n\n")));
            none()
        }
        "isfile" | "isfolder" | "isdir" => {
            let test = |p: &str| if name == "isfile" { Path::new(p).is_file() } else { Path::new(p).is_dir() };
            match arg(a, 0, name)? {
                Value::Cell(c) => one(Value::Num(Matrix::from_dims(c.dims.clone(), c.data.iter().map(|v| v.text().map(|t| if test(&t) { 1.0 } else { 0.0 })).collect::<R<_>>()?).cast_to(Kind::Logical))),
                Value::Str(s) if s.data.len() != 1 => one(Value::Num(Matrix::from_dims(s.dims.clone(), s.data.iter().map(|t| if test(t.as_deref().unwrap_or("")) { 1.0 } else { 0.0 }).collect()).cast_to(Kind::Logical))),
                v => one(Value::bool(test(&v.text()?))),
            }
        }
        "mkdir" => {
            let path = match a {
                [p] => PathBuf::from(p.text()?),
                [parent, p, ..] => Path::new(&parent.text()?).join(p.text()?),
                [] => return rt("Not enough input arguments."),
            };
            let (ok, msg, id) = if path.is_dir() {
                (true, "Directory already exists.", "MATLAB:MKDIR:DirectoryExists")
            } else {
                match std::fs::create_dir_all(&path) {
                    Ok(()) => (true, "", ""),
                    Err(_) => (false, "Permission denied.", "MATLAB:MKDIR:OSError"),
                }
            };
            if nargout == 0 {
                if !ok {
                    return rt(format!("Unable to create directory {}.", path.display()));
                }
                if !msg.is_empty() {
                    it.emit(OutputKind::Warning(msg.to_string()));
                }
                return none();
            }
            let txt = |s: &str| if s.is_empty() { Value::Num(Matrix::new(0, 0, Vec::new()).cast_to(Kind::Char)) } else { Value::chars(s) };
            Ok(Some(vec![Value::bool(ok), txt(msg), txt(id)].into_iter().take(nargout).collect()))
        }
        "rmdir" => {
            let p = arg(a, 0, name)?.text()?;
            let recursive = matches!(a.get(1), Some(v) if v.text().map(|t| t == "s").unwrap_or(false));
            let r = if recursive { std::fs::remove_dir_all(&p) } else { std::fs::remove_dir(&p) };
            match (r, nargout) {
                (Ok(()), 0) => none(),
                (Ok(()), _) => Ok(Some(vec![Value::bool(true), Value::chars("")].into_iter().take(nargout).collect())),
                (Err(_), 0) => rt(format!("{p} is not a folder or could not be removed (it may not be empty).")),
                (Err(e), _) => Ok(Some(vec![Value::bool(false), Value::chars(&e.to_string())].into_iter().take(nargout).collect())),
            }
        }
        "delete" => {
            for v in a {
                // Graphics handles are deleted by the graphics code; here only files.
                let pat = v.text()?;
                let hits = glob(&pat);
                if hits.is_empty() {
                    it.emit(OutputKind::Warning(format!("File '{pat}' not found.")));
                }
                for h in hits {
                    std::fs::remove_file(&h).map_err(io_err)?;
                }
            }
            none()
        }
        "copyfile" | "movefile" => {
            let src = arg(a, 0, name)?.text()?;
            let dst = match a.get(1) {
                Some(d) => PathBuf::from(d.text()?),
                None => PathBuf::from("."),
            };
            let sources = glob(&src);
            if sources.is_empty() {
                return rt(format!("No matching files were found."));
            }
            for s in sources {
                let target = if dst.is_dir() { dst.join(s.file_name().unwrap_or_default()) } else { dst.clone() };
                if s.is_dir() {
                    copy_dir(&s, &target).map_err(io_err)?;
                    if name == "movefile" {
                        std::fs::remove_dir_all(&s).map_err(io_err)?;
                    }
                } else if name == "movefile" {
                    std::fs::rename(&s, &target).or_else(|_| std::fs::copy(&s, &target).and_then(|_| std::fs::remove_file(&s))).map_err(io_err)?;
                } else {
                    std::fs::copy(&s, &target).map_err(io_err)?;
                }
            }
            if nargout > 0 {
                return Ok(Some(vec![Value::bool(true), Value::chars(""), Value::chars("")].into_iter().take(nargout).collect()));
            }
            none()
        }
        "fopen" => fopen(a, nargout),
        "fclose" => {
            let v = arg(a, 0, name)?;
            if v.is_text() && v.text()? == "all" {
                close_all();
                return one(Value::num(0.0));
            }
            let fid = v.scalar()? as i32;
            let closed = FILES.with(|t| t.borrow_mut().remove(&fid));
            match closed {
                Some(mut f) => {
                    f.flush();
                    // fclose always returns its status (ans = 0 when not suppressed).
                    one(Value::num(0.0))
                }
                None => rt("Invalid file identifier. Use fopen to generate a valid file identifier."),
            }
        }
        "fgetl" | "fgets" => {
            let fid = fid_arg(a, name)?;
            let line = with_file(fid, |f| Ok(next_line(f, name == "fgets")))?;
            one(match line {
                Some(l) => if l.is_empty() { Value::Num(Matrix::new(1, 0, Vec::new()).cast_to(Kind::Char)) } else { Value::chars(&l) },
                None => Value::num(-1.0),
            })
        }
        "feof" => {
            let fid = fid_arg(a, name)?;
            let eof = with_file(fid, |f| Ok(f.pos >= f.data.len()))?;
            one(Value::num(if eof { 1.0 } else { 0.0 }))
        }
        "ferror" => {
            let fid = fid_arg(a, name)?;
            let msg = with_file(fid, |f| Ok(std::mem::take(&mut f.last_error)))?;
            Ok(Some(vec![Value::chars(&msg), Value::num(0.0)].into_iter().take(nargout.max(1)).collect()))
        }
        "frewind" => {
            let fid = fid_arg(a, name)?;
            with_file(fid, |f| {
                f.pos = 0;
                Ok(())
            })?;
            none()
        }
        "ftell" => {
            let fid = fid_arg(a, name)?;
            one(Value::num(with_file(fid, |f| Ok(f.pos as f64))?))
        }
        "fseek" => {
            let fid = fid_arg(a, name)?;
            let off = arg(a, 1, name)?.scalar()?;
            let origin = match arg(a, 2, name)? {
                v if v.is_text() => match v.text()?.as_str() {
                    "bof" => -1.0,
                    "cof" => 0.0,
                    "eof" => 1.0,
                    o => return rt(format!("Invalid origin '{o}'.")),
                },
                v => v.scalar()?,
            };
            let st = with_file(fid, |f| {
                let base = if origin < 0.0 { 0.0 } else if origin == 0.0 { f.pos as f64 } else { f.data.len() as f64 };
                let np = base + off;
                if np < 0.0 || np > f.data.len() as f64 {
                    f.last_error = "Offset is bad - before beginning-of-file.".into();
                    return Ok(-1.0);
                }
                f.pos = np as usize;
                Ok(0.0)
            })?;
            one(Value::num(st))
        }
        "fread" => fread(a, nargout),
        "fwrite" => fwrite(it, a),
        "fscanf" => {
            let fid = fid_arg(a, name)?;
            let rest = with_file(fid, |f| Ok(String::from_utf8_lossy(&f.data[f.pos..]).into_owned()))?;
            let mut args = vec![Value::chars(&rest)];
            args.extend(a[1..].iter().cloned());
            let out = crate::builtins_strings::call(it, "sscanf", &args, 4)?.unwrap_or_default();
            // sscanf's fourth output is the next unread position (1-based, in characters).
            let next = out.get(3).map(|v| v.scalar()).transpose()?.unwrap_or(1.0) as usize;
            let consumed: usize = rest.chars().take(next.saturating_sub(1)).map(|c| c.len_utf8()).sum();
            with_file(fid, |f| {
                f.pos += consumed;
                Ok(())
            })?;
            Ok(Some(out.into_iter().take(nargout.max(1)).collect()))
        }
        "fileread" => one(Value::chars(&read_text_file(&arg(a, 0, name)?.text()?)?)),
        "readlines" => {
            let text = read_text_file(&arg(a, 0, name)?.text()?)?;
            let lines: Vec<Option<String>> = text.replace("\r\n", "\n").split('\n').map(|s| Some(s.to_string())).collect();
            one(Value::Str(StrArray::from_dims(vec![lines.len(), 1], lines)))
        }
        "writelines" => {
            let lines: Vec<String> = match arg(a, 0, name)? {
                Value::Cell(c) => c.data.iter().map(|v| v.text()).collect::<R<_>>()?,
                Value::Str(s) => s.data.iter().map(|t| t.clone().unwrap_or_default()).collect(),
                v => vec![v.text()?],
            };
            let path = arg(a, 1, name)?.text()?;
            let append = a.len() > 3 && a[2].text()?.eq_ignore_ascii_case("WriteMode") && a[3].text()?.eq_ignore_ascii_case("append");
            let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
            if append {
                use std::io::Write;
                std::fs::OpenOptions::new().create(true).append(true).open(&path).and_then(|mut f| f.write_all(body.as_bytes())).map_err(io_err)?;
            } else {
                std::fs::write(&path, body).map_err(io_err)?;
            }
            none()
        }
        "getenv" => {
            if a.is_empty() {
                // getenv() lists every variable as a dictionary in MATLAB; a struct here.
                let mut s = StructArray::scalar();
                for (k, v) in std::env::vars() {
                    if crate::builtins_lang::is_varname(&k) {
                        s.set(&k, Value::string(v));
                    }
                }
                return one(Value::Struct(s));
            }
            let (k, as_string) = text_in(&a[0])?;
            one(text_out(&std::env::var(&k).unwrap_or_default(), as_string))
        }
        "setenv" => {
            let k = arg(a, 0, name)?.text()?;
            let v = a.get(1).map(|v| v.text()).transpose()?.unwrap_or_default();
            // SAFETY: the interpreter is single-threaded with respect to the environment.
            unsafe { std::env::set_var(k, v) };
            none()
        }
        "unsetenv" => {
            let k = arg(a, 0, name)?.text()?;
            unsafe { std::env::remove_var(k) };
            none()
        }
        "isenv" => one(Value::bool(std::env::var_os(arg(a, 0, name)?.text()?).is_some())),
        "computer" => {
            let arch = if cfg!(windows) { "PCWIN64" } else if cfg!(target_os = "macos") { if cfg!(target_arch = "aarch64") { "MACA64" } else { "MACI64" } } else { "GLNXA64" };
            if matches!(a.first(), Some(v) if v.text().map(|t| t == "arch").unwrap_or(false)) {
                return one(Value::chars(&arch.to_lowercase()));
            }
            Ok(Some(vec![Value::chars(arch), Value::num(2f64.powi(48) - 1.0), Value::chars("L")].into_iter().take(nargout.max(1)).collect()))
        }
        "system" | "dos" | "unix" => {
            let cmd = arg(a, 0, name)?.text()?;
            let echo = a.iter().skip(1).any(|v| v.text().map(|t| t == "-echo").unwrap_or(false));
            let output = if cfg!(windows) { std::process::Command::new("cmd").args(["/C", &cmd]).output() } else { std::process::Command::new("sh").args(["-c", &cmd]).output() };
            let (status, text) = match output {
                Ok(o) => (o.status.code().unwrap_or(1) as f64, format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))),
                Err(e) => (1.0, e.to_string()),
            };
            let text = text.replace("\r\n", "\n");
            if nargout < 2 || echo {
                it.emit(OutputKind::Text(text.clone()));
            }
            Ok(Some(vec![Value::num(status), Value::chars(&text)].into_iter().take(nargout.max(1)).collect()))
        }
        _ => Ok(None),
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let target = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

fn char_rows(rows: &[String]) -> Value {
    let n = rows.len();
    let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let mut data = vec![32.0; n * w];
    for (i, r) in rows.iter().enumerate() {
        for (j, c) in r.chars().enumerate() {
            data[j * n + i] = c as u32 as f64;
        }
    }
    Value::Num(Matrix::new(n, w, data).cast_to(Kind::Char))
}

use crate::nd::Dims;
