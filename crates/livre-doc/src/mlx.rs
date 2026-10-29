//! `.mlx` reading and writing: an OPC (ZIP) package whose `matlab/document.xml` uses
//! WordprocessingML paragraphs (spec section 1).

use std::io::{Cursor, Read, Write};
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::{BlockKind, DocError, Document};

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
    loop {
        let ev = reader.read_event().map_err(|e| DocError::Format(format!("Malformed document.xml: {e}")))?;
        match ev {
            // Live-control markup inside mc:AlternateContent is skipped; its current value
            // is already part of the code text.
            Event::Start(e) if e.name().as_ref() == b"mc:AlternateContent" => alt_depth += 1,
            Event::Start(e) | Event::Empty(e) if alt_depth == 0 => {
                match e.name().as_ref() {
                    b"w:p" => {
                        in_p = true;
                        style = None;
                        sect = false;
                        text.clear();
                    }
                    b"w:pStyle" => {
                        for a in e.attributes().flatten() {
                            if a.key.as_ref() == b"w:val" {
                                style = Some(a.unescape_value().map(|v| v.into_owned()).unwrap_or_default());
                            }
                        }
                    }
                    b"w:sectPr" if in_p => sect = true,
                    b"w:t" => in_t = true,
                    b"w:br" if in_p => text.push('\n'),
                    b"w:tab" if in_p => text.push('\t'),
                    _ => {}
                }
            }
            Event::End(e) => match e.name().as_ref() {
                b"mc:AlternateContent" => alt_depth = alt_depth.saturating_sub(1),
                b"w:t" => in_t = false,
                b"w:p" if in_p => {
                    in_p = false;
                    if sect && text.trim().is_empty() {
                        doc.push(BlockKind::SectionBreak, "");
                    } else {
                        let kind = style_kind(style.as_deref());
                        let t = if kind == BlockKind::Code { text.replace("\r\n", "\n") } else { text.clone() };
                        doc.push(kind, t.trim_end_matches('\n').to_string());
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

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn cdata(s: &str) -> String {
    // A literal "]]>" must be split across two CDATA sections.
    format!("<![CDATA[{}]]>", s.replace("]]>", "]]]]><![CDATA[>"))
}

pub(crate) fn document_xml(doc: &Document) -> String {
    let mut x = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#);
    for b in &doc.blocks {
        match b.kind {
            BlockKind::SectionBreak => x.push_str("<w:p><w:pPr><w:sectPr/></w:pPr></w:p>"),
            BlockKind::Code => {
                x.push_str(r#"<w:p><w:pPr><w:pStyle w:val="code"/></w:pPr><w:r><w:t>"#);
                x.push_str(&cdata(&b.text));
                x.push_str("</w:t></w:r></w:p>");
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
}
