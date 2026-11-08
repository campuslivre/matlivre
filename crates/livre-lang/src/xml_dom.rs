//! `xmlread`, `xmlwrite` and `com.mathworks.xml.XMLUtils.createDocument`: XML documents as
//! the DOM objects MATLAB returns (Xerces documents, elements, text and comment nodes, node
//! lists and attribute maps) with their usual methods. Text comes back as char (MATLAB gives
//! java.lang.String, which `char` and `str2double` turn into the same values).

use crate::builtins::{arg, none, one, Ret};
use crate::class::{ClassDef, Object};
use crate::error::{rt, LangError};
use crate::interp::Interpreter;
use crate::value::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &["xmlread", "xmlwrite", "com.mathworks.xml.XMLUtils.createDocument", "matlivre_xml_method"];

/// The DOM methods the node classes answer (for the coverage checklist too).
const XML_METHODS: &[&str] = &[
    "getDocumentElement", "createElement", "createTextNode", "createComment", "getElementsByTagName", "getNodeName", "getNodeValue", "getNodeType",
    "getTextContent", "setTextContent", "getChildNodes", "getFirstChild", "getLastChild", "getNextSibling", "getPreviousSibling", "getParentNode",
    "hasChildNodes", "appendChild", "removeChild", "insertBefore", "getOwnerDocument", "getLength", "item", "getTagName", "getAttribute",
    "setAttribute", "hasAttribute", "removeAttribute", "getAttributes", "hasAttributes", "getName", "getValue", "getNamedItem", "getData",
    "toString",
];

#[derive(Clone)]
enum Kind {
    Document,
    Element { name: String, attrs: Vec<(String, String)> },
    Text(String),
    Comment(String),
}

#[derive(Clone)]
struct XNode {
    kind: Kind,
    parent: Option<usize>,
    children: Vec<usize>,
}

struct Dom {
    nodes: Vec<XNode>,
    /// Parsed from a file (Xerces' deferred classes) or built in code.
    parsed: bool,
}

thread_local! {
    static DOMS: RefCell<Vec<Dom>> = const { RefCell::new(Vec::new()) };
    static CLASSES: RefCell<HashMap<String, Rc<ClassDef>>> = RefCell::new(HashMap::new());
}

fn with_dom<T>(d: usize, f: impl FnOnce(&mut Dom) -> T) -> T {
    DOMS.with(|ds| f(&mut ds.borrow_mut()[d]))
}

fn class(name: &str) -> Rc<ClassDef> {
    CLASSES.with(|c| {
        c.borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| {
                let short = name.rsplit('.').next().unwrap_or(name);
                let mut src = format!("classdef {short}\n  properties (Hidden)\n    Doc\n    Node\n    Extra\n  end\n  methods\n");
                for m in XML_METHODS {
                    src.push_str(&format!("    function varargout = {m}(varargin)\n      varargout = matlivre_xml_method('{m}', varargin{{:}});\n    end\n"));
                }
                src.push_str("  end\nend\n");
                let ast = crate::class::parse_classdef(&src).expect("built-in class source parses");
                let mut def = ClassDef::build(ast, &[]);
                def.name = name.to_string();
                Rc::new(def)
            })
            .clone()
    })
}

/// What an object stands for: a node, a tag-name list under a node, or an attribute.
enum Handle {
    Node(usize, usize),
    List(usize, usize, String),
    Attrs(usize, usize),
    Attr(usize, usize, usize),
}

fn object(name: &str, d: usize, n: usize, extra: Value) -> Value {
    Value::Object(Object { class: class(name), props: Rc::new(RefCell::new(vec![Value::num(d as f64), Value::num(n as f64), extra])) })
}

fn node_value(d: usize, n: usize) -> Value {
    let (kind, parsed) = with_dom(d, |dom| (dom.nodes[n].kind.clone(), dom.parsed));
    let base = if parsed { "org.apache.xerces.dom.Deferred" } else { "org.apache.xerces.dom." };
    let cls = match kind {
        Kind::Document => "DocumentImpl",
        Kind::Element { .. } => "ElementImpl",
        Kind::Text(_) => "TextImpl",
        Kind::Comment(_) => "CommentImpl",
    };
    object(&format!("{base}{cls}"), d, n, Value::empty())
}

fn handle_of(v: &Value) -> Option<Handle> {
    let Value::Object(o) = v else { return None };
    if !o.class.name.starts_with("org.apache.xerces.dom.") {
        return None;
    }
    let d = o.get("Doc")?.scalar().ok()? as usize;
    let n = o.get("Node")?.scalar().ok()? as usize;
    let extra = o.get("Extra").unwrap_or_else(Value::empty);
    Some(match o.class.name.as_str() {
        "org.apache.xerces.dom.DeepNodeListImpl" => Handle::List(d, n, extra.text().unwrap_or_default()),
        "org.apache.xerces.dom.AttributeMap" => Handle::Attrs(d, n),
        c if c.ends_with("AttrImpl") => Handle::Attr(d, n, extra.scalar().unwrap_or(0.0) as usize),
        _ => Handle::Node(d, n),
    })
}

fn node_of(v: &Value) -> R<(usize, usize)> {
    match handle_of(v) {
        Some(Handle::Node(d, n)) => Ok((d, n)),
        _ => rt("Expected an XML node."),
    }
}

/// Java's null.
fn null() -> Value {
    Value::empty()
}

fn text_of(dom: &Dom, n: usize) -> String {
    match &dom.nodes[n].kind {
        Kind::Text(t) | Kind::Comment(t) => t.clone(),
        _ => dom.nodes[n].children.iter().filter(|&&c| !matches!(dom.nodes[c].kind, Kind::Comment(_))).map(|&c| text_of(dom, c)).collect(),
    }
}

/// Elements below `n` named `tag` (`*` for all), in document order.
fn descendants(dom: &Dom, n: usize, tag: &str, out: &mut Vec<usize>) {
    for &c in &dom.nodes[n].children {
        if let Kind::Element { name, .. } = &dom.nodes[c].kind {
            if tag == "*" || name == tag {
                out.push(c);
            }
        }
        descendants(dom, c, tag, out);
    }
}

fn add_node(d: usize, kind: Kind) -> usize {
    with_dom(d, |dom| {
        dom.nodes.push(XNode { kind, parent: None, children: Vec::new() });
        dom.nodes.len() - 1
    })
}

fn detach(dom: &mut Dom, c: usize) {
    if let Some(p) = dom.nodes[c].parent.take() {
        dom.nodes[p].children.retain(|&x| x != c);
    }
}

fn parse(text: &str) -> R<usize> {
    let doc = roxmltree::Document::parse(text).map_err(|e| LangError::new(format!("Failed to parse XML: {e}"), 0))?;
    let mut nodes = vec![XNode { kind: Kind::Document, parent: None, children: Vec::new() }];
    fn walk(n: roxmltree::Node, parent: usize, nodes: &mut Vec<XNode>) {
        for c in n.children() {
            let kind = if c.is_element() {
                Kind::Element { name: c.tag_name().name().to_string(), attrs: c.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect() }
            } else if c.is_text() {
                Kind::Text(c.text().unwrap_or_default().to_string())
            } else if c.is_comment() {
                Kind::Comment(c.text().unwrap_or_default().to_string())
            } else {
                continue;
            };
            nodes.push(XNode { kind, parent: Some(parent), children: Vec::new() });
            let id = nodes.len() - 1;
            nodes[parent].children.push(id);
            walk(c, id, nodes);
        }
    }
    walk(doc.root(), 0, &mut nodes);
    Ok(DOMS.with(|ds| {
        let mut ds = ds.borrow_mut();
        ds.push(Dom { nodes, parsed: true });
        ds.len() - 1
    }))
}

fn escape(s: &str, attr: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attr => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// The document as xmlwrite writes it: built documents indented by three spaces per level
/// (text stays on its element's line); parsed ones keep their own whitespace.
fn serialize(dom: &Dom) -> String {
    fn el(dom: &Dom, n: usize, depth: usize, out: &mut String) {
        match &dom.nodes[n].kind {
            Kind::Text(t) => out.push_str(&escape(t, false)),
            Kind::Comment(t) => out.push_str(&format!("<!--{t}-->")),
            Kind::Document => {}
            Kind::Element { name, attrs } => {
                out.push('<');
                out.push_str(name);
                for (k, v) in attrs {
                    out.push_str(&format!(" {k}=\"{}\"", escape(v, true)));
                }
                let kids = &dom.nodes[n].children;
                if kids.is_empty() {
                    out.push_str("/>");
                    return;
                }
                out.push('>');
                let has_elements = kids.iter().any(|&c| matches!(dom.nodes[c].kind, Kind::Element { .. }));
                let has_space = kids.iter().any(|&c| matches!(&dom.nodes[c].kind, Kind::Text(t) if t.trim().is_empty()));
                let pretty = has_elements && !has_space;
                // Elements and comments start their own lines, except right after a comment.
                let mut prev_comment = false;
                for &c in kids {
                    let k = &dom.nodes[c].kind;
                    if pretty && matches!(k, Kind::Element { .. } | Kind::Comment(_)) && !prev_comment {
                        out.push('\n');
                        out.push_str(&"   ".repeat(depth + 1));
                    }
                    prev_comment = matches!(k, Kind::Comment(_));
                    el(dom, c, depth + 1, out);
                }
                if pretty {
                    out.push('\n');
                    out.push_str(&"   ".repeat(depth));
                }
                out.push_str(&format!("</{name}>"));
            }
        }
    }
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>");
    for &c in &dom.nodes[0].children {
        out.push('\n');
        el(dom, c, 0, &mut out);
    }
    out
}

/// The display of an XML object: Java's `[name: value]`.
pub fn object_text(o: &Object) -> Option<String> {
    let h = handle_of(&Value::Object(o.clone()))?;
    Some(match h {
        Handle::Node(d, n) => with_dom(d, |dom| match &dom.nodes[n].kind {
            Kind::Document => "[#document: null]".to_string(),
            Kind::Element { name, .. } => format!("[{name}: null]"),
            Kind::Text(t) => format!("[#text: {t}]"),
            Kind::Comment(t) => format!("[#comment: {t}]"),
        }),
        Handle::List(..) | Handle::Attrs(..) => format!("{}@1", o.class.name),
        Handle::Attr(d, n, k) => with_dom(d, |dom| match &dom.nodes[n].kind {
            Kind::Element { attrs, .. } => attrs.get(k).map(|(a, v)| format!("{a}=\"{v}\"")).unwrap_or_default(),
            _ => String::new(),
        }),
    } + "\n")
}

fn method(a: &[Value]) -> R<Value> {
    let m = arg(a, 0, "matlivre_xml_method")?.text()?;
    let me = arg(a, 1, &m)?;
    let rest = &a[2..];
    let text_arg = |i: usize| -> R<String> { arg(rest, i, &m)?.text() };
    let h = handle_of(me).ok_or_else(|| LangError::new("Expected an XML object.", 0))?;
    // Lists and attribute maps.
    match h {
        Handle::List(d, n, tag) => {
            let items = with_dom(d, |dom| {
                let mut v = Vec::new();
                descendants(dom, n, &tag, &mut v);
                v
            });
            return match m.as_str() {
                "getLength" => Ok(Value::num(items.len() as f64)),
                "item" => Ok(items.get(arg(rest, 0, &m)?.scalar()? as usize).map(|&c| node_value(d, c)).unwrap_or_else(null)),
                _ => rt(format!("No method '{m}' for a node list.")),
            };
        }
        Handle::Attrs(d, n) => {
            let names: Vec<String> = with_dom(d, |dom| match &dom.nodes[n].kind {
                Kind::Element { attrs, .. } => attrs.iter().map(|x| x.0.clone()).collect(),
                _ => Vec::new(),
            });
            let attr = |k: usize| object("org.apache.xerces.dom.DeferredAttrImpl", d, n, Value::num(k as f64));
            return match m.as_str() {
                "getLength" => Ok(Value::num(names.len() as f64)),
                "item" => {
                    let k = arg(rest, 0, &m)?.scalar()? as usize;
                    Ok(if k < names.len() { attr(k) } else { null() })
                }
                "getNamedItem" => {
                    let want = text_arg(0)?;
                    Ok(names.iter().position(|x| *x == want).map(attr).unwrap_or_else(null))
                }
                _ => rt(format!("No method '{m}' for an attribute map.")),
            };
        }
        Handle::Attr(d, n, k) => {
            let (name, value) = with_dom(d, |dom| match &dom.nodes[n].kind {
                Kind::Element { attrs, .. } => attrs.get(k).cloned().unwrap_or_default(),
                _ => Default::default(),
            });
            return match m.as_str() {
                "getName" | "getNodeName" => Ok(Value::chars(&name)),
                "getValue" | "getNodeValue" | "getTextContent" => Ok(Value::chars(&value)),
                "getNodeType" => Ok(Value::num(2.0)),
                _ => rt(format!("No method '{m}' for an attribute.")),
            };
        }
        Handle::Node(..) => {}
    }
    let Handle::Node(d, n) = h else { unreachable!() };
    let child_arg = |i: usize| -> R<usize> {
        let (dd, c) = node_of(arg(rest, i, &m)?)?;
        if dd != d {
            return rt("WRONG_DOCUMENT_ERR: A node is used in a different document than the one that created it.");
        }
        Ok(c)
    };
    let kind = with_dom(d, |dom| dom.nodes[n].kind.clone());
    let nav = |f: &dyn Fn(&Dom) -> Option<usize>| with_dom(d, |dom| f(dom)).map(|c| node_value(d, c)).unwrap_or_else(null);
    Ok(match m.as_str() {
        "getDocumentElement" => nav(&|dom: &Dom| dom.nodes[0].children.iter().copied().find(|&c| matches!(dom.nodes[c].kind, Kind::Element { .. }))),
        "createElement" => node_value(d, add_node(d, Kind::Element { name: text_arg(0)?, attrs: Vec::new() })),
        "createTextNode" => node_value(d, add_node(d, Kind::Text(text_arg(0)?))),
        "createComment" => node_value(d, add_node(d, Kind::Comment(text_arg(0)?))),
        "getElementsByTagName" => object("org.apache.xerces.dom.DeepNodeListImpl", d, n, Value::chars(&text_arg(0)?)),
        "getNodeName" | "getTagName" => Value::chars(&match &kind {
            Kind::Document => "#document".to_string(),
            Kind::Element { name, .. } => name.clone(),
            Kind::Text(_) => "#text".to_string(),
            Kind::Comment(_) => "#comment".to_string(),
        }),
        "getNodeValue" | "getData" => match &kind {
            Kind::Text(t) | Kind::Comment(t) => Value::chars(t),
            _ => null(),
        },
        "getNodeType" => Value::num(match kind {
            Kind::Element { .. } => 1.0,
            Kind::Text(_) => 3.0,
            Kind::Comment(_) => 8.0,
            Kind::Document => 9.0,
        }),
        "getTextContent" => match kind {
            Kind::Document => null(),
            _ => Value::chars(&with_dom(d, |dom| text_of(dom, n))),
        },
        "setTextContent" => {
            let t = text_arg(0)?;
            let id = add_node(d, Kind::Text(t.clone()));
            with_dom(d, |dom| match &mut dom.nodes[n].kind {
                Kind::Text(x) | Kind::Comment(x) => *x = t,
                _ => {
                    for c in std::mem::take(&mut dom.nodes[n].children) {
                        dom.nodes[c].parent = None;
                    }
                    dom.nodes[id].parent = Some(n);
                    dom.nodes[n].children.push(id);
                }
            });
            return Ok(Value::Cell(CellArray::new(1, 0, Vec::new())));
        }
        // An element is its own list of children, as in Xerces.
        "getChildNodes" => me.clone(),
        "getLength" => Value::num(with_dom(d, |dom| dom.nodes[n].children.len()) as f64),
        "item" => {
            let k = arg(rest, 0, &m)?.scalar()? as usize;
            nav(&|dom: &Dom| dom.nodes[n].children.get(k).copied())
        }
        "getFirstChild" => nav(&|dom: &Dom| dom.nodes[n].children.first().copied()),
        "getLastChild" => nav(&|dom: &Dom| dom.nodes[n].children.last().copied()),
        "getParentNode" => nav(&|dom: &Dom| dom.nodes[n].parent),
        "getNextSibling" | "getPreviousSibling" => nav(&|dom: &Dom| {
            let p = dom.nodes[n].parent?;
            let sib = &dom.nodes[p].children;
            let i = sib.iter().position(|&x| x == n)?;
            if m == "getNextSibling" { sib.get(i + 1).copied() } else { i.checked_sub(1).map(|j| sib[j]) }
        }),
        "getOwnerDocument" => node_value(d, 0),
        "hasChildNodes" => Value::bool(with_dom(d, |dom| !dom.nodes[n].children.is_empty())),
        "appendChild" | "insertBefore" => {
            let c = child_arg(0)?;
            let before = if m == "insertBefore" { arg(rest, 1, &m).ok().and_then(|v| node_of(v).ok()).map(|x| x.1) } else { None };
            with_dom(d, |dom| {
                detach(dom, c);
                let at = before.and_then(|b| dom.nodes[n].children.iter().position(|&x| x == b)).unwrap_or(dom.nodes[n].children.len());
                dom.nodes[n].children.insert(at, c);
                dom.nodes[c].parent = Some(n);
            });
            node_value(d, c)
        }
        "removeChild" => {
            let c = child_arg(0)?;
            with_dom(d, |dom| detach(dom, c));
            node_value(d, c)
        }
        "getAttribute" | "hasAttribute" | "setAttribute" | "removeAttribute" => {
            let key = text_arg(0)?;
            let val = if m == "setAttribute" { Some(text_arg(1)?) } else { None };
            let out = with_dom(d, |dom| {
                let Kind::Element { attrs, .. } = &mut dom.nodes[n].kind else { return Err(()) };
                let i = attrs.iter().position(|x| x.0 == key);
                Ok(match m.as_str() {
                    "getAttribute" => Value::chars(&i.map(|i| attrs[i].1.clone()).unwrap_or_default()),
                    "hasAttribute" => Value::bool(i.is_some()),
                    "setAttribute" => {
                        let v = val.clone().unwrap_or_default();
                        match i {
                            Some(i) => attrs[i].1 = v,
                            None => attrs.push((key.clone(), v)),
                        }
                        Value::Cell(CellArray::new(1, 0, Vec::new()))
                    }
                    _ => {
                        if let Some(i) = i {
                            attrs.remove(i);
                        }
                        Value::Cell(CellArray::new(1, 0, Vec::new()))
                    }
                })
            });
            match out {
                Ok(Value::Cell(c)) if c.data.is_empty() => return Ok(Value::Cell(c)),
                Ok(v) => v,
                Err(()) => return rt(format!("No method '{m}' for this kind of node.")),
            }
        }
        "getAttributes" => match kind {
            Kind::Element { .. } => object("org.apache.xerces.dom.AttributeMap", d, n, Value::empty()),
            _ => null(),
        },
        "hasAttributes" => Value::bool(matches!(&kind, Kind::Element { attrs, .. } if !attrs.is_empty())),
        "toString" => Value::chars(object_text(&match me {
            Value::Object(o) => o.clone(),
            _ => unreachable!(),
        })
        .unwrap_or_default()
        .trim_end()),
        _ => return rt(format!("No method '{m}' for this kind of node.")),
    })
}

pub fn call(_it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "xmlread" => {
            let path = arg(a, 0, name)?.text()?;
            let text = std::fs::read_to_string(&path).map_err(|_| LangError::new(format!("Failed to read XML file {path}."), 0))?;
            let d = parse(&text)?;
            one(node_value(d, 0))
        }
        "com.mathworks.xml.XMLUtils.createDocument" => {
            let root = arg(a, 0, name)?.text()?;
            let d = DOMS.with(|ds| {
                let mut ds = ds.borrow_mut();
                ds.push(Dom {
                    nodes: vec![
                        XNode { kind: Kind::Document, parent: None, children: vec![1] },
                        XNode { kind: Kind::Element { name: root, attrs: Vec::new() }, parent: Some(0), children: Vec::new() },
                    ],
                    parsed: false,
                });
                ds.len() - 1
            });
            one(node_value(d, 0))
        }
        "xmlwrite" => {
            // xmlwrite(doc) returns the text; xmlwrite(file, doc) writes it.
            let (file, doc) = match a {
                [doc] => (None, doc),
                [f, doc, ..] => (Some(f.text()?), doc),
                [] => return rt("Not enough input arguments."),
            };
            let (d, n) = node_of(doc)?;
            let text = with_dom(d, |dom| {
                if n == 0 {
                    serialize(dom)
                } else {
                    // A single node: a document of that subtree.
                    let mut sub = Dom { nodes: dom.nodes.clone(), parsed: dom.parsed };
                    sub.nodes[0].children = vec![n];
                    serialize(&sub)
                }
            });
            match file {
                Some(f) => {
                    std::fs::write(&f, text + "\n").map_err(|e| LangError::new(format!("Unable to write '{f}': {e}"), 0))?;
                    none()
                }
                None => {
                    let _ = nargout;
                    one(Value::chars(&text))
                }
            }
        }
        // Methods without a result (setAttribute, ...) give an empty cell: no outputs.
        "matlivre_xml_method" => Ok(Some(vec![match method(a)? {
            Value::Cell(c) if c.data.is_empty() => Value::Cell(CellArray::new(1, 0, Vec::new())),
            v => Value::Cell(CellArray::row(vec![v])),
        }])),
        _ => Ok(None),
    }
}
