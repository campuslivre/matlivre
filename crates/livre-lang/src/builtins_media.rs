//! Images (`imread`, `imwrite`, `imfinfo`) and WAV audio (`audioread`, `audiowrite`,
//! `audioinfo`, `lin2mu`, `mu2lin`).
//!
//! Images are height×width (grayscale) or height×width×3 arrays, uint8 for 8-bit files and
//! uint16 for 16-bit PNGs; 1-bit PNGs read as logical. PNG is handled directly (bit depths,
//! palettes, alpha); JPEG, BMP, GIF and TIFF through the `image` crate.

use std::path::Path;

use crate::builtins::{arg, none, one, Ret};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["imread", "imwrite", "imfinfo", "audioread", "audiowrite", "audioinfo", "lin2mu", "mu2lin"];

fn err(e: impl std::fmt::Display) -> LangError {
    LangError::new(e.to_string(), 0)
}

fn ext_of(path: &str) -> String {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// A decoded image: rows × cols × channels samples (row-major), the sample class, and the
/// palette for indexed images.
struct Image {
    w: usize,
    h: usize,
    channels: usize,
    samples: Vec<f64>,
    kind: Kind,
    alpha: Option<Vec<f64>>,
    palette: Option<Vec<[u8; 3]>>,
    bit_depth: u32,
    color_type: &'static str,
}

fn read_png(path: &str) -> R<Image> {
    let file = std::fs::File::open(path).map_err(|_| LangError::new(format!("File \"{path}\" does not exist."), 0))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(err)?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).map_err(err)?;
    let info = reader.info();
    let (w, h) = (frame.width as usize, frame.height as usize);
    let depth = frame.bit_depth as u8 as u32;
    let line = frame.line_size;
    // Unpack samples of `depth` bits from each row.
    let channels_in = frame.color_type.samples();
    let read_row = |y: usize| -> Vec<f64> {
        let row = &buf[y * line..(y + 1) * line];
        let n = w * channels_in;
        match depth {
            16 => (0..n).map(|i| u16::from_be_bytes([row[2 * i], row[2 * i + 1]]) as f64).collect(),
            8 => row[..n].iter().map(|&b| b as f64).collect(),
            d => (0..n).map(|i| {
                let bit = i * d as usize;
                let byte = row[bit / 8];
                let shift = 8 - d as usize - bit % 8;
                ((byte >> shift) & ((1u16 << d) - 1) as u8) as f64
            }).collect(),
        }
    };
    let rows: Vec<Vec<f64>> = (0..h).map(read_row).collect();
    let mut samples = Vec::with_capacity(w * h * 3);
    let mut alpha = None;
    let (channels, color_type, palette) = match frame.color_type {
        png::ColorType::Grayscale => {
            rows.iter().for_each(|r| samples.extend(r));
            (1, "grayscale", None)
        }
        png::ColorType::GrayscaleAlpha => {
            let mut a = Vec::new();
            for r in &rows {
                for p in r.chunks(2) {
                    samples.push(p[0]);
                    a.push(p[1]);
                }
            }
            alpha = Some(a);
            (1, "grayscale", None)
        }
        png::ColorType::Rgb => {
            rows.iter().for_each(|r| samples.extend(r));
            (3, "truecolor", None)
        }
        png::ColorType::Rgba => {
            let mut a = Vec::new();
            for r in &rows {
                for p in r.chunks(4) {
                    samples.extend(&p[..3]);
                    a.push(p[3]);
                }
            }
            alpha = Some(a);
            (3, "truecolor", None)
        }
        png::ColorType::Indexed => {
            rows.iter().for_each(|r| samples.extend(r));
            let pal = info.palette.as_ref().map(|p| p.chunks(3).map(|c| [c[0], c[1], c[2]]).collect());
            (1, "indexed", pal)
        }
    };
    let kind = match depth {
        16 => Kind::Int(IntClass::U16),
        1 if frame.color_type == png::ColorType::Grayscale => Kind::Logical,
        _ => Kind::Int(IntClass::U8),
    };
    Ok(Image { w, h, channels, samples, kind, alpha, palette, bit_depth: depth * channels_in as u32, color_type })
}

fn read_other(path: &str) -> R<Image> {
    let img = image::open(path).map_err(|e| match e {
        image::ImageError::IoError(_) => LangError::new(format!("File \"{path}\" does not exist."), 0),
        other => err(other),
    })?;
    let (w, h) = (img.width() as usize, img.height() as usize);
    let gray = matches!(img.color(), image::ColorType::L8 | image::ColorType::L16 | image::ColorType::La8 | image::ColorType::La16);
    let sixteen = matches!(img.color(), image::ColorType::L16 | image::ColorType::Rgb16 | image::ColorType::Rgba16);
    let (channels, samples): (usize, Vec<f64>) = match (gray, sixteen) {
        (true, false) => (1, img.to_luma8().into_raw().into_iter().map(|b| b as f64).collect()),
        (true, true) => (1, img.to_luma16().into_raw().into_iter().map(|b| b as f64).collect()),
        (false, false) => (3, img.to_rgb8().into_raw().into_iter().map(|b| b as f64).collect()),
        (false, true) => (3, img.to_rgb16().into_raw().into_iter().map(|b| b as f64).collect()),
    };
    let alpha = img.color().has_alpha().then(|| img.to_rgba8().pixels().map(|p| p.0[3] as f64).collect());
    let depth = if sixteen { 16 } else { 8 };
    Ok(Image {
        w,
        h,
        channels,
        samples,
        kind: if sixteen { Kind::Int(IntClass::U16) } else { Kind::Int(IntClass::U8) },
        alpha,
        palette: None,
        bit_depth: depth * channels as u32,
        color_type: if gray { "grayscale" } else { "truecolor" },
    })
}

fn read_image(path: &str) -> R<Image> {
    if ext_of(path) == "png" { read_png(path) } else { read_other(path) }
}

/// Row-major interleaved samples to a column-major h×w(×c) array.
fn to_array(img: &Image) -> Matrix {
    let (w, h, c) = (img.w, img.h, img.channels);
    let mut data = vec![0.0; w * h * c];
    for y in 0..h {
        for x in 0..w {
            for k in 0..c {
                data[k * w * h + x * h + y] = img.samples[(y * w + x) * c + k];
            }
        }
    }
    let dims = if c == 1 { vec![h, w] } else { vec![h, w, c] };
    Matrix::from_dims(dims, data).cast_to(img.kind)
}

fn imread(a: &[Value], nargout: usize) -> Ret {
    let path = arg(a, 0, "imread")?.text()?;
    let img = read_image(&path)?;
    let mut out = vec![Value::Num(to_array(&img))];
    if nargout > 1 {
        out.push(match &img.palette {
            Some(p) => {
                let n = p.len();
                let mut data = vec![0.0; n * 3];
                for (i, c) in p.iter().enumerate() {
                    for k in 0..3 {
                        data[k * n + i] = c[k] as f64 / 255.0;
                    }
                }
                Value::Num(Matrix::new(n, 3, data))
            }
            None => Value::Num(Matrix::new(0, 0, Vec::new())),
        });
    }
    if nargout > 2 {
        out.push(match &img.alpha {
            Some(a) => {
                let alpha_img = Image { w: img.w, h: img.h, channels: 1, samples: a.clone(), kind: img.kind, alpha: None, palette: None, bit_depth: 0, color_type: "" };
                Value::Num(to_array(&alpha_img))
            }
            None => Value::Num(Matrix::new(0, 0, Vec::new())),
        });
    }
    Ok(Some(out))
}

/// Samples of an array to write: (height, width, channels, 8- or 16-bit values, bits).
fn samples_for_write(m: &Matrix) -> R<(usize, usize, usize, Vec<u16>, u32)> {
    let dims = &m.dims;
    let (h, w) = (dims[0], dims.get(1).copied().unwrap_or(1));
    let c = dims.get(2).copied().unwrap_or(1);
    if c != 1 && c != 3 && c != 4 {
        return rt("Image data must be either MxN (grayscale) or MxNx3 (truecolor).");
    }
    let (bits, conv): (u32, Box<dyn Fn(f64) -> u16>) = match m.kind {
        Kind::Logical => (1, Box::new(|x| if x != 0.0 { 1 } else { 0 })),
        Kind::Int(IntClass::U16) => (16, Box::new(|x| x as u16)),
        Kind::Int(IntClass::U8) | Kind::Char => (8, Box::new(|x| x as u8 as u16)),
        // Double and single data run 0..1 and scale to 0..255.
        Kind::Double | Kind::Single => (8, Box::new(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u16)),
        Kind::Int(_) => (8, Box::new(|x| x.clamp(0.0, 255.0) as u16)),
    };
    let mut out = Vec::with_capacity(w * h * c);
    for y in 0..h {
        for x in 0..w {
            for k in 0..c {
                out.push(conv(m.data[k * w * h + x * h + y]));
            }
        }
    }
    Ok((h, w, c, out, bits))
}

fn write_png(path: &str, h: usize, w: usize, c: usize, s: &[u16], bits: u32) -> R<()> {
    let file = std::fs::File::create(path).map_err(err)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(match c {
        1 => png::ColorType::Grayscale,
        3 => png::ColorType::Rgb,
        _ => png::ColorType::Rgba,
    });
    enc.set_depth(match bits {
        1 => png::BitDepth::One,
        16 => png::BitDepth::Sixteen,
        _ => png::BitDepth::Eight,
    });
    let mut writer = enc.write_header().map_err(err)?;
    let data: Vec<u8> = match bits {
        16 => s.iter().flat_map(|v| v.to_be_bytes()).collect(),
        1 => {
            let row_bytes = w.div_ceil(8);
            let mut out = vec![0u8; row_bytes * h];
            for y in 0..h {
                for x in 0..w {
                    if s[y * w + x] != 0 {
                        out[y * row_bytes + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
            out
        }
        _ => s.iter().map(|&v| v as u8).collect(),
    };
    writer.write_image_data(&data).map_err(err)
}

fn imwrite(a: &[Value]) -> Ret {
    let m = arg(a, 0, "imwrite")?.to_matrix()?;
    // imwrite(A, file), imwrite(A, map, file) (indexed: written as RGB), imwrite(A, file, fmt).
    let (m, path_at) = match a.get(1) {
        Some(v) if !v.is_text() && !matches!(v, Value::Str(_)) => {
            let map = v.to_matrix()?;
            let n = map.rows();
            let (h, w) = (m.dims[0], m.dims.get(1).copied().unwrap_or(1));
            let offset = if matches!(m.kind, Kind::Double | Kind::Single) { 1.0 } else { 0.0 };
            let mut data = vec![0.0; h * w * 3];
            for i in 0..h * w {
                let idx = ((m.data[i] - offset).max(0.0) as usize).min(n.saturating_sub(1));
                for k in 0..3 {
                    data[k * h * w + i] = map.data[k * n + idx];
                }
            }
            (Matrix::from_dims(vec![h, w, 3], data), 2)
        }
        _ => (m, 1),
    };
    let path = arg(a, path_at, "imwrite")?.text()?;
    let fmt = match a.get(path_at + 1) {
        Some(v) if v.is_text() && !matches!(v.text()?.to_lowercase().as_str(), "quality" | "alpha" | "bitdepth") => v.text()?.to_lowercase(),
        _ => ext_of(&path),
    };
    let (h, w, c, s, bits) = samples_for_write(&m)?;
    if fmt == "png" {
        write_png(&path, h, w, c, &s, bits)?;
        return none();
    }
    let bytes: Vec<u8> = s.iter().map(|&v| if bits == 1 { (v * 255) as u8 } else if bits == 16 { (v >> 8) as u8 } else { v as u8 }).collect();
    let img: image::DynamicImage = match c {
        1 => image::GrayImage::from_raw(w as u32, h as u32, bytes).map(image::DynamicImage::ImageLuma8),
        3 => image::RgbImage::from_raw(w as u32, h as u32, bytes).map(image::DynamicImage::ImageRgb8),
        _ => image::RgbaImage::from_raw(w as u32, h as u32, bytes).map(image::DynamicImage::ImageRgba8),
    }
    .ok_or_else(|| LangError::new("Invalid image data.", 0))?;
    let format = match fmt.as_str() {
        "jpg" | "jpeg" => image::ImageFormat::Jpeg,
        "bmp" => image::ImageFormat::Bmp,
        "gif" => image::ImageFormat::Gif,
        "tif" | "tiff" => image::ImageFormat::Tiff,
        other => return rt(format!("Unable to determine the file format from the file name, or the format '{other}' is not supported.")),
    };
    img.save_with_format(&path, format).map_err(err)?;
    none()
}

fn imfinfo(a: &[Value]) -> Ret {
    let path = arg(a, 0, "imfinfo")?.text()?;
    let img = read_image(&path)?;
    let meta = std::fs::metadata(&path).map_err(err)?;
    let mut s = StructArray::scalar();
    let full = std::fs::canonicalize(&path).map(|p| p.display().to_string().trim_start_matches(r"\\?\").to_string()).unwrap_or(path.clone());
    s.set("Filename", Value::chars(&full));
    let dn = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as f64).unwrap_or(0.0) + crate::time::local_offset_ms();
    s.set("FileModDate", Value::chars(&crate::time::format_datetime(dn, "dd-MMM-yyyy HH:mm:ss")));
    s.set("FileSize", Value::num(meta.len() as f64));
    let ext = ext_of(&path);
    s.set("Format", Value::chars(if ext == "jpeg" { "jpg" } else if ext == "tiff" { "tif" } else { &ext }));
    s.set("FormatVersion", Value::chars(""));
    s.set("Width", Value::num(img.w as f64));
    s.set("Height", Value::num(img.h as f64));
    s.set("BitDepth", Value::num(img.bit_depth as f64));
    s.set("ColorType", Value::chars(img.color_type));
    one(Value::Struct(s))
}

/// G.711 mu-law of a 16-bit sample.
fn mu_encode(sample: i32) -> u8 {
    const BIAS: i32 = 0x84;
    const CLIP: i32 = 32635;
    let sign = if sample < 0 { 0x80 } else { 0 };
    let mut s = sample.abs().min(CLIP) + BIAS;
    let mut exponent = 7;
    let mut mask = 0x4000;
    while exponent > 0 && s & mask == 0 {
        exponent -= 1;
        mask >>= 1;
    }
    let mantissa = (s >> (exponent + 3)) & 0x0f;
    s = !(sign | (exponent << 4) | mantissa);
    (s & 0xff) as u8
}

fn mu_decode(u: u8) -> i32 {
    let u = !u as i32;
    let sign = u & 0x80;
    let exponent = (u >> 4) & 0x07;
    let mantissa = u & 0x0f;
    let s = (((mantissa << 3) + 0x84) << exponent) - 0x84;
    if sign != 0 { -s } else { s }
}

fn audioread(a: &[Value], nargout: usize) -> Ret {
    let path = arg(a, 0, "audioread")?.text()?;
    let native = a.iter().skip(1).any(|v| v.text().map(|t| t == "native").unwrap_or(false));
    let range = a.get(1).filter(|v| !v.is_text()).map(|v| v.to_matrix()).transpose()?;
    let mut reader = hound::WavReader::open(&path).map_err(|e| match e {
        hound::Error::IoError(_) => LangError::new(format!("The filename specified was not found in the MATLAB path."), 0),
        other => err(other),
    })?;
    let spec = reader.spec();
    let ch = spec.channels as usize;
    let (vals, kind): (Vec<f64>, Kind) = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, _) => (reader.samples::<f32>().map(|s| s.map(|v| v as f64)).collect::<Result<_, _>>().map_err(err)?, Kind::Single),
        (hound::SampleFormat::Int, 8) => (reader.samples::<i32>().map(|s| s.map(|v| v as f64)).collect::<Result<_, _>>().map_err(err)?, Kind::Int(IntClass::U8)),
        (hound::SampleFormat::Int, 16) => (reader.samples::<i32>().map(|s| s.map(|v| v as f64)).collect::<Result<_, _>>().map_err(err)?, Kind::Int(IntClass::I16)),
        (hound::SampleFormat::Int, b) => (reader.samples::<i32>().map(|s| s.map(|v| v as f64)).collect::<Result<_, _>>().map_err(err)?, if b <= 16 { Kind::Int(IntClass::I16) } else { Kind::Int(IntClass::I32) }),
    };
    let scale = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, _) => 1.0,
        (_, 8) => 128.0,
        (_, b) => 2f64.powi(b as i32 - 1),
    };
    let frames = vals.len() / ch.max(1);
    let (first, last) = match &range {
        Some(r) if r.numel() == 2 => (r.data[0] as usize - 1, (r.data[1] as usize).min(frames)),
        _ => (0, frames),
    };
    let n = last.saturating_sub(first);
    let mut data = vec![0.0; n * ch];
    for f in 0..n {
        for c in 0..ch {
            let v = vals[(first + f) * ch + c];
            data[c * n + f] = if native { v } else if spec.bits_per_sample == 8 && spec.sample_format == hound::SampleFormat::Int { (v - 128.0) / scale } else { v / scale };
        }
    }
    let m = Matrix::new(n, ch, data);
    let m = if native { m.cast_to(kind) } else { m };
    let mut out = vec![Value::Num(m)];
    if nargout > 1 {
        out.push(Value::num(spec.sample_rate as f64));
    }
    Ok(Some(out))
}

fn audiowrite(a: &[Value]) -> Ret {
    let path = arg(a, 0, "audiowrite")?.text()?;
    let y = arg(a, 1, "audiowrite")?.to_matrix()?;
    let fs = arg(a, 2, "audiowrite")?.scalar()?;
    let mut bits = 16u16;
    let mut i = 3;
    while i + 1 < a.len() {
        if a[i].text()?.eq_ignore_ascii_case("BitsPerSample") {
            bits = a[i + 1].scalar()? as u16;
        }
        i += 2;
    }
    if ext_of(&path) != "wav" {
        return rt("Only WAV files can be written in this version.");
    }
    let (n, ch) = if y.rows() == 1 && y.cols() > 1 && y.dims.len() == 2 { (y.cols(), 1) } else { (y.rows(), y.cols()) };
    let float = bits == 32 && matches!(y.kind, Kind::Double | Kind::Single) || bits == 64;
    let spec = hound::WavSpec {
        channels: ch as u16,
        sample_rate: fs as u32,
        bits_per_sample: if bits == 64 { 32 } else { bits },
        sample_format: if float { hound::SampleFormat::Float } else { hound::SampleFormat::Int },
    };
    let mut w = hound::WavWriter::create(&path, spec).map_err(err)?;
    let full = 2f64.powi(bits as i32 - 1);
    for f in 0..n {
        for c in 0..ch {
            let v = y.data[c * n + f];
            if float {
                w.write_sample(v as f32).map_err(err)?;
            } else {
                // Doubles in -1..1 scale to the integer range (rounding down, as MATLAB does), clipped.
                let s = if matches!(y.kind, Kind::Double | Kind::Single) { (v * full).floor().clamp(-full, full - 1.0) } else { v };
                if bits == 8 {
                    w.write_sample((s + 128.0) as i8 as i32 - 128).map_err(err)?;
                } else {
                    w.write_sample(s as i32).map_err(err)?;
                }
            }
        }
    }
    w.finalize().map_err(err)?;
    none()
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match name {
        "imread" => imread(a, nargout),
        "imwrite" => imwrite(a),
        "imfinfo" => imfinfo(a),
        "audioread" => audioread(a, nargout),
        "audiowrite" => audiowrite(a),
        "audioinfo" => {
            let path = arg(a, 0, name)?.text()?;
            let reader = hound::WavReader::open(&path).map_err(err)?;
            let spec = reader.spec();
            let total = reader.duration() as f64;
            let mut s = StructArray::scalar();
            s.set("Filename", Value::chars(&std::fs::canonicalize(&path).map(|p| p.display().to_string().trim_start_matches(r"\\?\").to_string()).unwrap_or(path.clone())));
            s.set("CompressionMethod", Value::chars("Uncompressed"));
            s.set("NumChannels", Value::num(spec.channels as f64));
            s.set("SampleRate", Value::num(spec.sample_rate as f64));
            s.set("TotalSamples", Value::num(total));
            s.set("Duration", Value::num(total / spec.sample_rate as f64));
            s.set("Title", Value::empty());
            s.set("Comment", Value::empty());
            s.set("Artist", Value::empty());
            s.set("BitsPerSample", Value::num(spec.bits_per_sample as f64));
            one(Value::Struct(s))
        }
        "lin2mu" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            one(Value::Num(m.map(|x| mu_encode((x * 32768.0).round() as i32) as f64)))
        }
        "mu2lin" => {
            let m = arg(a, 0, name)?.to_matrix()?;
            one(Value::Num(m.map(|x| mu_decode(x as u8) as f64 / 32768.0)))
        }
        _ => Ok(None),
    }
}
