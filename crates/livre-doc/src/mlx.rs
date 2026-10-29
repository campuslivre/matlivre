//! `.mlx` reading and writing: an OPC (ZIP) package whose `matlab/document.xml` uses
//! WordprocessingML paragraphs (spec section 1).

use std::io::{Cursor, Read, Write};
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::controls::span_bytes;
use crate::json::Json;
use crate::{Block, BlockKind, ControlKind, DocError, Document, LiveControl};

const DOCUMENT_PART: &str = "matlab/document.xml";

pub fn read_mlx(path: &Path) -> Result<Document, DocError> {
    read_mlx_bytes(&std::fs::read(path)?)
}

pub fn read_mlx_bytes(bytes: &[u8]) -> Result<Document, DocError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| DocError::Format(format!("Not a live script package: {e}")))?;
    let mut xml = String::new();
    zip.by_name(DOCUMENT_PART)
        .map_err(|_| DocError::Format(format!("The package has no {DOCUMENT_PART} part.")))?
        .read_to_string(&mut xml)?;
    parse_document_xml(&xml)
}

fn style_kind(style: Option<&str>) -> BlockKind {
    match style {
        Some("code") | Some("CodeExampleLine") => BlockKind::Code,
        Some("title") => BlockKind::Title,
        Some("heading") | Some("TOCHeading") => BlockKind::Heading,
        Some("heading2") | Some("heading3") => BlockKind::Heading2,
        _ => BlockKind::Text,
    }
}

pub(crate) fn parse_document_xml(xml: &str) -> Result<Document, DocError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut doc = Document::new();
    let mut in_p = false;
    let mut in_t = false;
    let mut alt_depth = 0usize;
    let mut style: Option<String> = None;
    let mut sect = false;
    let mut text = String::new();
    let mut in_choice = false;
    let mut in_control = false;
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut raw_controls: Vec<Vec<(String, String)>> = Vec::new();
    loop {
        let ev = reader.read_event().map_err(|e| DocError::Format(format!("Malformed document.xml: {e}")))?;
        match ev {
            // Inside mc:AlternateContent only live controls (in mc:Choice) are read; their
            // current value is already part of the code text.
            Event::Start(e) if e.name().as_ref() == b"mc:AlternateContent" => alt_depth += 1,
            Event::Start(e) if alt_depth > 0 => match e.name().as_ref() {
                b"mc:Choice" => in_choice = true,
                b"w:customXml" if in_choice && in_p && attr(&e, b"w:element").as_deref() == Some("livecontrol") => {
                    in_control = true;
                    attrs.clear();
                }
                b"w:attr" if in_control => attrs.extend(name_val(&e)),
                _ => {}
            },
            Event::Empty(e) if alt_depth > 0 && in_control && e.name().as_ref() == b"w:attr" => attrs.extend(name_val(&e)),
            Event::Start(e) | Event::Empty(e) if alt_depth == 0 => {
                match e.name().as_ref() {
                    b"w:p" => {
                        in_p = true;
                        style = None;
                        sect = false;
                        text.clear();
                        raw_controls.clear();
                    }
                    b"w:pStyle" => style = Some(attr(&e, b"w:val").unwrap_or_default()),
                    b"w:sectPr" if in_p => sect = true,
                    b"w:t" => in_t = true,
                    b"w:br" if in_p => text.push('\n'),
                    b"w:tab" if in_p => text.push('\t'),
                    _ => {}
                }
            }
            Event::End(e) => match e.name().as_ref() {
                b"mc:AlternateContent" => alt_depth = alt_depth.saturating_sub(1),
                b"mc:Choice" => in_choice = false,
                b"w:customXml" if in_control => {
                    in_control = false;
                    raw_controls.push(std::mem::take(&mut attrs));
                }
                b"w:t" => in_t = false,
                b"w:p" if in_p => {
                    in_p = false;
                    if sect && text.trim().is_empty() {
                        doc.push(BlockKind::SectionBreak, "");
                    } else {
                        let kind = style_kind(style.as_deref());
                        let t = if kind == BlockKind::Code { text.replace("\r\n", "\n") } else { text.clone() };
                        doc.push(kind, t.trim_end_matches('\n').to_string());
                        if kind == BlockKind::Code {
                            let b = doc.blocks.last_mut().expect("just pushed");
                            for raw in raw_controls.drain(..) {
                                if let Some(c) = decode_control(raw, &b.text) {
                                    b.add_control(c);
                                }
                            }
                        }
                        if sect {
                            doc.push(BlockKind::SectionBreak, "");
                        }
                    }
                }
                _ => {}
            },
            Event::Text(t) if in_t && alt_depth == 0 => {
                text.push_str(&t.unescape().map_err(|e| DocError::Format(e.to_string()))?);
            }
            Event::CData(c) if in_t && alt_depth == 0 => {
                text.push_str(&String::from_utf8_lossy(&c));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if doc.blocks.is_empty() {
        doc.push(BlockKind::Code, "");
    }
    Ok(doc)
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == key).map(|a| a.unescape_value().map(|v| v.into_owned()).unwrap_or_default())
}

fn name_val(e: &quick_xml::events::BytesStart) -> Option<(String, String)> {
    Some((attr(e, b"w:name")?, attr(e, b"w:val").unwrap_or_default()))
}

const SPAN_ATTRS: [&str; 4] = ["startOffsetLine", "startColumn", "endOffsetLine", "endColumn"];

/// Canonical `context.type` for a type name as found in files (case-insensitive).
fn canonical_type(ty: &str) -> Option<&'static str> {
    Some(match ty.to_ascii_lowercase().as_str() {
        "slider" => "slider",
        "combobox" | "dropdown" => "comboBox",
        "checkbox" => "checkBox",
        "editfield" => "editField",
        "button" => "button",
        _ => return None,
    })
}

fn kind_type(kind: &ControlKind) -> &'static str {
    match kind {
        ControlKind::Slider { .. } => "slider",
        ControlKind::DropDown { .. } => "comboBox",
        ControlKind::CheckBox => "checkBox",
        ControlKind::EditField => "editField",
        ControlKind::Button { .. } => "button",
    }
}

fn json_text(v: &Json) -> Option<String> {
    match v {
        Json::Str(s) => Some(s.clone()),
        Json::Num(_) | Json::Bool(_) => Some(v.to_string()),
        _ => None,
    }
}

/// Code literals (and labels) of drop-down items given as strings, numbers or `{value, label}` objects.
fn items(v: Option<&Json>) -> Vec<(String, Option<String>)> {
    let Some(Json::Arr(a)) = v else { return Vec::new() };
    a.iter()
        .filter_map(|it| match it {
            Json::Obj(_) => {
                let val = it.get("value").or_else(|| it.get("item")).and_then(json_text)?;
                Some((val, it.get("label").or_else(|| it.get("text")).and_then(json_text)))
            }
            _ => json_text(it).map(|t| (t, None)),
        })
        .collect()
}

/// A live control from its customXml attributes, if its type is known and its span matches `code`.
fn decode_control(attrs: Vec<(String, String)>, code: &str) -> Option<LiveControl> {
    let get = |n: &str| attrs.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
    let ctx = Json::parse(get("context")?)?;
    let data = ctx.get("data");
    let field = |k: &str| data.and_then(|d| d.get(k)).or_else(|| ctx.get(k));
    let num = |k: &str, d: f64| field(k).and_then(Json::as_f64).filter(|v| v.is_finite()).unwrap_or(d);
    let kind = match canonical_type(ctx.get("type")?.as_str()?)? {
        "slider" => {
            let (min, max, step) = (num("min", 0.0), num("max", 100.0), num("step", 1.0));
            ControlKind::Slider { min: min.min(max), max: max.max(min), step: if step > 0.0 { step } else { 1.0 } }
        }
        "comboBox" => {
            let mut it = items(field("items"));
            let labels = field("itemLabels").or_else(|| field("labels")).map(|l| items(Some(l)));
            if let Some(labels) = labels.filter(|l| l.len() == it.len()) {
                for (i, l) in it.iter_mut().zip(labels) {
                    i.1 = Some(l.0);
                }
            }
            let labels = it.iter().map(|(v, l)| l.clone().unwrap_or_else(|| v.clone())).collect();
            ControlKind::DropDown { items: it.into_iter().map(|(v, _)| v).collect(), labels }
        }
        "checkBox" => ControlKind::CheckBox,
        "editField" => ControlKind::EditField,
        _ => ControlKind::Button { label: field("label").or_else(|| field("text")).and_then(json_text).unwrap_or_else(|| "Run".into()) },
    };
    let int = |n: &str| get(n).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.fract() == 0.0 && *v >= 0.0).map(|v| v as usize);
    let (sl, sc) = (int("startOffsetLine")?, int("startColumn")?);
    if int("endOffsetLine").is_some_and(|el| el != sl) {
        return None;
    }
    let start = sc.checked_sub(1)?;
    let lit = get("text");
    let button = matches!(kind, ControlKind::Button { .. });
    // A 1-based line and an exclusive end column first; when there is a `text` to check the
    // span against, also an inclusive end column, the literal's length and a 0-based line.
    let ec = int("endColumn");
    let mut lines: Vec<usize> = sl.checked_sub(1).into_iter().collect();
    let mut ends: Vec<usize> = if button { vec![start] } else { ec.and_then(|e| e.checked_sub(1)).into_iter().collect() };
    if let (Some(lit), false) = (lit, button) {
        ends.extend(ec);
        ends.push(start + lit.chars().count());
        lines.push(sl);
    }
    let (line, end) = lines.iter().flat_map(|&l| ends.iter().map(move |&e| (l, e))).find(|&(l, e)| {
        (button || e > start) && span_bytes(code, l, start, e).is_some_and(|r| button || lit.is_none_or(|t| code[r] == *t))
    })?;
    let extra = attrs.iter().filter(|(k, _)| k != "text" && !SPAN_ATTRS.contains(&k.as_str())).cloned().collect();
    Some(LiveControl { kind, line, start, end, extra })
}

/// The `context` JSON of control `i`, updating its original JSON when that is of the same type.
fn context_json(b: &Block, i: usize) -> String {
    let c = &b.controls[i];
    let ty = kind_type(&c.kind);
    let base = c.extra.iter().find(|(k, _)| k == "context").and_then(|(_, v)| Json::parse(v));
    let mut ctx = base
        .filter(|j| matches!(j, Json::Obj(_)) && j.get("type").and_then(Json::as_str).and_then(canonical_type) == Some(ty))
        .unwrap_or_else(|| Json::Obj(vec![("type".into(), Json::Str(ty.into()))]));
    let mut data = ctx.get("data").filter(|d| matches!(d, Json::Obj(_))).cloned().unwrap_or(Json::Obj(Vec::new()));
    let lit = b.control_text(i);
    let strs = |v: &[String]| Json::Arr(v.iter().map(|s| Json::Str(s.clone())).collect());
    match &c.kind {
        ControlKind::Slider { min, max, step } => {
            data.set("min", Json::Num(*min));
            data.set("max", Json::Num(*max));
            data.set("step", Json::Num(*step));
            data.set("value", lit.trim().parse().map_or_else(|_| Json::Str(lit.into()), Json::Num));
        }
        ControlKind::DropDown { items, labels } => {
            data.set("items", strs(items));
            let key = if data.get("labels").is_some() { "labels" } else { "itemLabels" };
            if labels != items || data.get(key).is_some() {
                data.set(key, strs(labels));
            }
            data.set("value", Json::Str(lit.into()));
        }
        ControlKind::CheckBox => data.set("value", Json::Bool(lit.trim() == "true")),
        ControlKind::EditField => data.set("value", Json::Str(lit.into())),
        ControlKind::Button { label } => data.set("label", Json::Str(label.clone())),
    }
    ctx.set("data", data);
    ctx.to_string()
}

fn control_xml(b: &Block, i: usize) -> String {
    let c = &b.controls[i];
    let mut x = String::from(r#"<mc:AlternateContent><mc:Choice Requires="R2018a"><w:customXml w:element="livecontrol"><w:customXmlPr>"#);
    let mut push = |k: &str, v: &str| x.push_str(&format!(r#"<w:attr w:name="{}" w:val="{}"/>"#, attr_esc(k), attr_esc(v)));
    push("context", &context_json(b, i));
    push("text", b.control_text(i));
    for (k, v) in SPAN_ATTRS.iter().zip([c.line + 1, c.start + 1, c.line + 1, c.end + 1]) {
        push(k, &v.to_string());
    }
    for (k, v) in c.extra.iter().filter(|(k, _)| k != "context") {
        push(k, v);
    }
    x.push_str("</w:customXmlPr></w:customXml></mc:Choice><mc:Fallback/></mc:AlternateContent>");
    x
}

fn attr_esc(s: &str) -> String {
    esc(s).replace('\n', "&#10;").replace('\r', "&#13;").replace('\t', "&#9;")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn cdata(s: &str) -> String {
    // A literal "]]>" must be split across two CDATA sections.
    format!("<![CDATA[{}]]>", s.replace("]]>", "]]]]><![CDATA[>"))
}

pub(crate) fn document_xml(doc: &Document) -> String {
    let mut x = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><w:body>"#);
    for b in &doc.blocks {
        match b.kind {
            BlockKind::SectionBreak => x.push_str("<w:p><w:pPr><w:sectPr/></w:pPr></w:p>"),
            BlockKind::Code => {
                x.push_str(r#"<w:p><w:pPr><w:pStyle w:val="code"/></w:pPr><w:r><w:t>"#);
                x.push_str(&cdata(&b.text));
                x.push_str("</w:t></w:r>");
                for i in 0..b.controls.len() {
                    x.push_str(&control_xml(b, i));
                }
                x.push_str("</w:p>");
            }
            kind => {
                let style = match kind {
                    BlockKind::Title => "title",
                    BlockKind::Heading => "heading",
                    BlockKind::Heading2 => "heading2",
                    _ => "text",
                };
                x.push_str(&format!(r#"<w:p><w:pPr><w:pStyle w:val="{style}"/><w:jc w:val="left"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>"#, esc(&b.text)));
            }
        }
    }
    x.push_str("</w:body></w:document>");
    x
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default ContentType="application/vnd.openxmlformats-package.relationships+xml" Extension="rels"/><Default ContentType="application/vnd.mathworks.matlab.code.document+xml" Extension="xml"/><Override ContentType="text/xml" PartName="/matlab/output.xml"/><Override ContentType="application/vnd.openxmlformats-package.core-properties+xml" PartName="/metadata/coreProperties.xml"/><Override ContentType="application/vnd.mathworks.package.coreProperties+xml" PartName="/metadata/mwcoreProperties.xml"/><Override ContentType="application/vnd.mathworks.package.corePropertiesExtension+xml" PartName="/metadata/mwcorePropertiesExtension.xml"/></Types>"#;

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Target="matlab/document.xml" Type="http://schemas.mathworks.com/matlab/code/2013/relationships/document"/><Relationship Id="rId2" Target="matlab/output.xml" Type="http://schemas.mathworks.com/matlab/code/2013/relationships/output"/><Relationship Id="rId3" Target="metadata/mwcoreProperties.xml" Type="http://schemas.mathworks.com/package/2012/relationships/coreProperties"/><Relationship Id="rId4" Target="metadata/mwcorePropertiesExtension.xml" Type="http://schemas.mathworks.com/package/2014/relationships/corePropertiesExtension"/><Relationship Id="rId5" Target="metadata/coreProperties.xml" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"/></Relationships>"#;

const OUTPUT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?><embeddedOutputs><metaData><evaluationState>manual</evaluationState><layoutState>code</layoutState><outputStatus>ready</outputStatus></metaData><outputArray type="array"/><regionArray type="array"/></embeddedOutputs>"#;

const MW_CORE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><mwcoreProperties xmlns="http://schemas.mathworks.com/package/2012/coreProperties"><contentType>application/vnd.mathworks.matlab.code</contentType><contentTypeFriendlyName>MATLAB Code</contentTypeFriendlyName><matlabRelease>R2023b</matlabRelease></mwcoreProperties>"#;

const MW_CORE_EXT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><mwcoreProperties xmlns="http://schemas.mathworks.com/package/2014/corePropertiesExtension"><matlabVersion>23.2.0.0</matlabVersion></mwcoreProperties>"#;

/// Current UTC time as W3CDTF (`2026-10-28T12:00:00Z`).
fn utc_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", sod / 3600, (sod % 3600) / 60, sod % 60)
}

pub fn write_mlx_bytes(doc: &Document) -> Result<Vec<u8>, DocError> {
    let now = utc_now();
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dcterms:created xsi:type="dcterms:W3CDTF">{now}</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">{now}</dcterms:modified></cp:coreProperties>"#
    );
    let parts: [(&str, String); 7] = [
        ("[Content_Types].xml", CONTENT_TYPES.into()),
        ("_rels/.rels", RELS.into()),
        (DOCUMENT_PART, document_xml(doc)),
        ("matlab/output.xml", OUTPUT_XML.into()),
        ("metadata/coreProperties.xml", core),
        ("metadata/mwcoreProperties.xml", MW_CORE.into()),
        ("metadata/mwcorePropertiesExtension.xml", MW_CORE_EXT.into()),
    ];
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in parts {
            zw.start_file(name, opts).map_err(|e| DocError::Format(e.to_string()))?;
            zw.write_all(body.as_bytes())?;
        }
        zw.finish().map_err(|e| DocError::Format(e.to_string()))?;
    }
    Ok(buf.into_inner())
}

pub fn write_mlx(doc: &Document, path: &Path) -> Result<(), DocError> {
    let bytes = write_mlx_bytes(doc)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_vocabulary() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:pStyle w:val="title"/><w:jc w:val="left"/></w:pPr><w:r><w:t>Solar</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="text"/></w:pPr><w:r><w:t>Some </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="code"/></w:pPr><w:r><w:t><![CDATA[x = 1;
y = 2;]]></w:t></w:r><mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="R2018a"><w:customXml w:element="livecontrol"><w:customXmlPr><w:attr w:name="text" w:val="1"/></w:customXmlPr></w:customXml></mc:Choice><mc:Fallback/></mc:AlternateContent></w:p><w:p><w:pPr><w:sectPr/></w:pPr></w:p><w:p><w:pPr><w:pStyle w:val="heading"/></w:pPr><w:r><w:t>Next</w:t></w:r></w:p></w:body></w:document>"#;
        let d = parse_document_xml(xml).unwrap();
        let got: Vec<(BlockKind, &str)> = d.blocks.iter().map(|b| (b.kind, b.text.as_str())).collect();
        assert_eq!(
            got,
            vec![
                (BlockKind::Title, "Solar"),
                (BlockKind::Text, "Some bold"),
                (BlockKind::Code, "x = 1;\ny = 2;"),
                (BlockKind::SectionBreak, ""),
                (BlockKind::Heading, "Next"),
            ]
        );
    }

    #[test]
    fn package_round_trip() {
        let mut d = Document::new();
        d.push(BlockKind::Title, "A & B <test>");
        d.push(BlockKind::Code, "s = ']]>';\ndisp(s)");
        d.push(BlockKind::SectionBreak, "");
        d.push(BlockKind::Text, "done");
        let bytes = write_mlx_bytes(&d).unwrap();
        let back = read_mlx_bytes(&bytes).unwrap();
        let a: Vec<(BlockKind, &str)> = d.blocks.iter().map(|b| (b.kind, b.text.as_str())).collect();
        let b: Vec<(BlockKind, &str)> = back.blocks.iter().map(|b| (b.kind, b.text.as_str())).collect();
        assert_eq!(a, b);
    }

    fn spans(b: &Block) -> Vec<(ControlKind, usize, usize, usize, &str)> {
        (0..b.controls.len()).map(|i| (b.controls[i].kind.clone(), b.controls[i].line, b.controls[i].start, b.controls[i].end, b.control_text(i))).collect()
    }

    #[test]
    fn minimal_control_without_context_keeps_code_only() {
        let xml = r#"<w:document xmlns:w="w"><w:body><w:p><w:pPr><w:pStyle w:val="code"/></w:pPr><w:r><w:t><![CDATA[x = 1;]]></w:t></w:r><mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="R2018a"><w:customXml w:element="livecontrol"><w:customXmlPr><w:attr w:name="text" w:val="1"/></w:customXmlPr></w:customXml></mc:Choice><mc:Fallback/></mc:AlternateContent></w:p></w:body></w:document>"#;
        let d = parse_document_xml(xml).unwrap();
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].text, "x = 1;");
        assert!(d.blocks[0].controls.is_empty());
    }

    #[test]
    fn reads_controls_leniently() {
        let ctl = |attrs: &[(&str, &str)]| {
            let a: String = attrs.iter().map(|(k, v)| format!(r#"<w:attr w:name="{k}" w:val="{}"/>"#, attr_esc(v))).collect();
            format!(r#"<mc:AlternateContent xmlns:mc="m"><mc:Choice Requires="R2018a"><w:customXml w:element="livecontrol"><w:customXmlPr>{a}</w:customXmlPr></w:customXml></mc:Choice><mc:Fallback><w:r><w:t>junk</w:t></w:r></mc:Fallback></mc:AlternateContent>"#)
        };
        let span = |l: &'static str, s: &'static str, e: &'static str| [("startOffsetLine", l), ("startColumn", s), ("endOffsetLine", l), ("endColumn", e)];
        let slider_ctx = r#"{"type":"slider","data":{"min":1,"max":10,"value":3,"runOn":"change"}}"#;
        let controls = [
            // Inclusive end column, no step.
            ctl(&[&[("context", slider_ctx), ("text", "3"), ("id", "abc")][..], &span("1", "8", "8")].concat()),
            ctl(&[&[("context", r#"{"type":"dropdown","data":{"items":[{"value":"'sin'","label":"Sine"},{"value":"'cos'","label":"Cosine"}]}}"#), ("text", "'sin'")][..], &span("2", "6", "11")].concat()),
            ctl(&[&[("context", r#"{"type":"checkbox","data":{"value":true}}"#), ("text", "true")][..], &span("2", "18", "22")].concat()),
            // Unknown type, a span that does not match its text, and no text at all.
            ctl(&[&[("context", r#"{"type":"colorPicker"}"#), ("text", "plot")][..], &span("3", "1", "5")].concat()),
            ctl(&[&[("context", r#"{"type":"editField"}"#), ("text", "freq2")][..], &span("3", "6", "10")].concat()),
            ctl(&[&[("context", r#"{"type":"editField"}"#)][..], &span("3", "6", "10")].concat()),
        ]
        .concat();
        let xml = format!(
            r#"<w:document xmlns:w="w"><w:body><w:p><w:pPr><w:pStyle w:val="code"/></w:pPr><w:r><w:t><![CDATA[freq = 3;
fn = 'sin'; on = true;
plot(freq)]]></w:t></w:r>{controls}</w:p></w:body></w:document>"#
        );
        let d = parse_document_xml(&xml).unwrap();
        let b = &d.blocks[0];
        assert_eq!(b.text, "freq = 3;\nfn = 'sin'; on = true;\nplot(freq)");
        let dd = ControlKind::DropDown { items: vec!["'sin'".into(), "'cos'".into()], labels: vec!["Sine".into(), "Cosine".into()] };
        assert_eq!(
            spans(b),
            vec![
                (ControlKind::Slider { min: 1.0, max: 10.0, step: 1.0 }, 0, 7, 8, "3"),
                (dd, 1, 5, 10, "'sin'"),
                (ControlKind::CheckBox, 1, 17, 21, "true"),
                (ControlKind::EditField, 2, 5, 9, "freq"),
            ]
        );
        assert_eq!(b.controls[0].extra, vec![("context".to_string(), slider_ctx.to_string()), ("id".to_string(), "abc".to_string())]);
        // Unknown context fields survive a write.
        let out = document_xml(&d);
        assert!(out.contains(&attr_esc(r#"{"type":"slider","data":{"min":1,"max":10,"value":3,"runOn":"change","step":1}}"#)), "{out}");
    }

    #[test]
    fn controls_round_trip() {
        let mut d = Document::new();
        d.push(BlockKind::Title, "Controls");
        let code = "freq = 2.5; fn = 'sin';\nshow = true; name = \"a<b&\"\"c\";\nplot(freq)";
        d.push(BlockKind::Code, code);
        assert!(d.add_control(1, 0, 7, 10, ControlKind::Slider { min: -1.0, max: 10.0, step: 0.5 }));
        assert!(d.add_control(1, 0, 17, 22, ControlKind::DropDown { items: vec!["'sin'".into(), "'cos'".into()], labels: vec!["Sine".into(), "Cosine".into()] }));
        assert!(d.add_control(1, 1, 7, 11, ControlKind::CheckBox));
        assert!(d.add_control(1, 1, 20, 29, ControlKind::EditField));
        assert!(d.add_control(1, 2, 10, 10, ControlKind::Button { label: "Run \"it\"\n<now>".into() }));
        d.blocks[1].controls[3].extra.push(("note".into(), "a\tb\n\"c\" & <d>".into()));
        let back = read_mlx_bytes(&write_mlx_bytes(&d).unwrap()).unwrap();
        assert_eq!(back.blocks[1].text, code);
        assert_eq!(spans(&back.blocks[1]), spans(&d.blocks[1]));
        assert_eq!(back.blocks[1].control_text(3), "\"a<b&\"\"c\"");
        for (a, b) in d.blocks[1].controls.iter().zip(&back.blocks[1].controls) {
            assert_eq!(a.extra[..], b.extra[1..]);
            assert_eq!(b.extra[0].0, "context");
        }
        // Writing what was read reproduces the same document.
        assert_eq!(document_xml(&back), document_xml(&d));
        let mut changed = back.clone();
        changed.blocks[1].set_control_value(0, "7");
        let again = read_mlx_bytes(&write_mlx_bytes(&changed).unwrap()).unwrap();
        assert_eq!(again.blocks[1].text, "freq = 7; fn = 'sin';\nshow = true; name = \"a<b&\"\"c\";\nplot(freq)");
        assert_eq!(again.blocks[1].control_text(1), "'sin'");
    }
}
