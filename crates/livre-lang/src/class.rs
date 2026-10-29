//! `classdef` support: parsing class files, the resolved class model (with inheritance),
//! objects with handle or value semantics, and the default object display.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{Expr, Function};
use crate::display::{summary, NumFormat};
use crate::error::LangError;
use crate::lexer::{tokenize, Kw, Tok};
use crate::value::Value;

type R<T> = Result<T, LangError>;

#[derive(Debug, Clone)]
pub struct PropDef {
    pub name: String,
    pub default: Option<Expr>,
    pub constant: bool,
    pub dependent: bool,
    pub hidden: bool,
}

#[derive(Debug)]
pub struct MethodDef {
    pub func: Rc<Function>,
    pub is_static: bool,
}

#[derive(Debug)]
pub struct ClassAst {
    pub name: String,
    pub supers: Vec<String>,
    pub props: Vec<PropDef>,
    pub methods: Vec<MethodDef>,
}

/// A loaded class with inherited members merged in.
#[derive(Debug)]
pub struct ClassDef {
    pub name: String,
    pub supers: Vec<String>,
    /// All superclasses, transitively.
    pub ancestors: Vec<String>,
    pub is_handle: bool,
    pub props: Vec<PropDef>,
    /// Method name → (function, defining class).
    pub methods: HashMap<String, (Rc<Function>, String)>,
    pub statics: HashSet<String>,
    /// The class's own constructor (not inherited).
    pub ctor: Option<Rc<Function>>,
}

impl ClassDef {
    pub fn prop_index(&self, name: &str) -> Option<usize> {
        self.props.iter().position(|p| p.name == name)
    }

    pub fn method(&self, name: &str) -> Option<&Rc<Function>> {
        self.methods.get(name).map(|(f, _)| f)
    }

    pub fn is_a(&self, class: &str) -> bool {
        self.name == class || self.ancestors.iter().any(|a| a == class)
    }

    /// Resolves a class from its AST and already-loaded superclasses.
    pub fn build(ast: ClassAst, supers: &[Rc<ClassDef>]) -> ClassDef {
        let mut props: Vec<PropDef> = Vec::new();
        let mut methods: HashMap<String, (Rc<Function>, String)> = HashMap::new();
        let mut statics = HashSet::new();
        let mut ancestors = Vec::new();
        let mut is_handle = false;
        for s in supers {
            is_handle |= s.is_handle;
            for a in std::iter::once(&s.name).chain(s.ancestors.iter()) {
                if !ancestors.contains(a) {
                    ancestors.push(a.clone());
                }
            }
            for p in &s.props {
                if !props.iter().any(|q| q.name == p.name) {
                    props.push(p.clone());
                }
            }
            for (k, v) in &s.methods {
                methods.entry(k.clone()).or_insert_with(|| v.clone());
            }
            statics.extend(s.statics.iter().cloned());
        }
        for s in &ast.supers {
            if s == "handle" {
                is_handle = true;
                if !ancestors.contains(s) {
                    ancestors.push(s.clone());
                }
            }
        }
        for p in ast.props {
            match props.iter_mut().find(|q| q.name == p.name) {
                Some(slot) => *slot = p,
                None => props.push(p),
            }
        }
        let mut ctor = None;
        for m in ast.methods {
            let name = m.func.name.clone();
            if name == ast.name {
                ctor = Some(m.func);
                continue;
            }
            if m.is_static {
                statics.insert(name.clone());
            }
            methods.insert(name, (m.func, ast.name.clone()));
        }
        ClassDef { name: ast.name, supers: ast.supers, ancestors, is_handle, props, methods, statics, ctor }
    }
}

/// An instance of a classdef class. Handle objects share `props`; value objects copy on
/// write.
#[derive(Debug, Clone)]
pub struct Object {
    pub class: Rc<ClassDef>,
    pub props: Rc<RefCell<Vec<Value>>>,
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        if self.class.is_handle {
            Rc::ptr_eq(&self.props, &other.props)
        } else {
            self.class.name == other.class.name && *self.props.borrow() == *other.props.borrow()
        }
    }
}

impl Object {
    pub fn get(&self, name: &str) -> Option<Value> {
        self.class.prop_index(name).map(|i| self.props.borrow()[i].clone())
    }

    /// Sets a property, copying first for value classes that share storage.
    pub fn set(mut self, name: &str, v: Value) -> R<Object> {
        let Some(i) = self.class.prop_index(name) else {
            return Err(LangError::new(format!("Unrecognized property '{name}' for class '{}'.", self.class.name), 0));
        };
        let p = &self.class.props[i];
        if p.constant {
            return Err(LangError::new(format!("Unable to set the '{name}' property of class '{}' because it is a Constant property.", self.class.name), 0));
        }
        if p.dependent {
            return Err(LangError::new(format!("Unable to set the '{name}' property of class '{}' because it is Dependent and has no set method in this version.", self.class.name), 0));
        }
        if !self.class.is_handle && Rc::strong_count(&self.props) > 1 {
            let copy = self.props.borrow().clone();
            self.props = Rc::new(RefCell::new(copy));
        }
        self.props.borrow_mut()[i] = v;
        Ok(self)
    }
}

/// Default display: `obj = \n  Foo with properties:\n\n    Name: 'x'`. `values` may carry
/// computed Dependent values (otherwise the stored ones are shown).
pub fn format_object(name: &str, o: &Object, values: &[Value], f: NumFormat) -> String {
    let visible: Vec<usize> = (0..o.class.props.len()).filter(|&i| !o.class.props[i].hidden && !o.class.props[i].constant).collect();
    let w = visible.iter().map(|&i| o.class.props[i].name.len()).max().unwrap_or(0);
    let mut out = format!("{name} = \n  {} with properties:\n\n", o.class.name);
    if visible.is_empty() {
        return format!("{name} = \n  {} with no properties.", o.class.name);
    }
    for i in visible {
        out.push_str(&format!("    {:>w$}: {}\n", o.class.props[i].name, summary(&values[i], f)));
    }
    out.trim_end().to_string()
}

pub fn display_object(name: &str, o: &Object, f: NumFormat) -> String {
    let values = o.props.borrow().clone();
    format_object(name, o, &values, f)
}

// ---- classdef parsing ----------------------------------------------------------------------

fn strip_comment(line: &str) -> &str {
    let mut in_str: Option<char> = None;
    let mut prev = ' ';
    for (i, c) in line.char_indices() {
        match in_str {
            Some(q) if c == q => in_str = None,
            Some(_) => {}
            None => {
                if c == '%' {
                    return &line[..i];
                }
                if c == '"' || (c == '\'' && !(prev.is_alphanumeric() || prev == ')' || prev == ']' || prev == '_')) {
                    in_str = Some(c);
                }
            }
        }
        prev = c;
    }
    line
}

pub fn is_classdef(src: &str) -> bool {
    for line in src.lines() {
        let t = strip_comment(line).trim();
        if t.is_empty() {
            continue;
        }
        return t == "classdef" || t.starts_with("classdef ") || t.starts_with("classdef(");
    }
    false
}

fn err(msg: impl Into<String>, line: usize) -> LangError {
    LangError::new(msg, line)
}

/// Parses `classdef (Attrs) Name < Super1 & Super2`.
fn parse_header(text: &str, line: usize) -> R<(String, Vec<String>)> {
    let mut rest = text.trim().strip_prefix("classdef").unwrap_or("").trim_start();
    if rest.starts_with('(') {
        let close = rest.find(')').ok_or_else(|| err("Invalid classdef attributes.", line))?;
        rest = rest[close + 1..].trim_start();
    }
    let (name, supers) = match rest.split_once('<') {
        Some((n, s)) => (n.trim(), s.split('&').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()),
        None => (rest.trim(), Vec::new()),
    };
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(err("Invalid class name in classdef.", line));
    }
    Ok((name.to_string(), supers))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Block {
    Classdef,
    Properties,
    Methods,
    Skip,
    Other,
}

fn attrs_of(header: &str) -> String {
    match (header.find('('), header.rfind(')')) {
        (Some(a), Some(b)) if b > a => header[a + 1..b].to_string(),
        _ => String::new(),
    }
}

fn has_attr(attrs: &str, name: &str) -> bool {
    attrs.split(',').any(|a| {
        let a = a.trim();
        match a.split_once('=') {
            Some((k, v)) => k.trim().eq_ignore_ascii_case(name) && v.trim().eq_ignore_ascii_case("true"),
            None => a.eq_ignore_ascii_case(name),
        }
    })
}

fn access_private(attrs: &str) -> bool {
    attrs.split(',').any(|a| match a.split_once('=') {
        Some((k, v)) => {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim().trim_matches('\'').to_ascii_lowercase();
            (k == "access" || k == "getaccess") && (v == "private" || v == "protected")
        }
        None => false,
    })
}

pub fn parse_classdef(src: &str) -> R<ClassAst> {
    let toks = tokenize(src)?;
    let lines: Vec<&str> = src.lines().collect();
    let line_text = |l: usize| strip_comment(lines.get(l.wrapping_sub(1)).copied().unwrap_or(""));
    let mut i = 0;
    while i < toks.len() && matches!(toks[i].tok, Tok::Newline | Tok::Sep(_)) {
        i += 1;
    }
    match toks.get(i).map(|t| &t.tok) {
        Some(Tok::Ident(s)) if s == "classdef" => {}
        _ => return Err(err("A classdef file must start with 'classdef'.", 1)),
    }
    let header_line = toks[i].line;
    let (name, supers) = parse_header(line_text(header_line), header_line)?;
    // Skip the rest of the header line.
    while i < toks.len() && toks[i].line == header_line {
        i += 1;
    }
    let mut stack: Vec<(Block, usize)> = vec![(Block::Classdef, header_line)];
    let mut depth = 0i32; // bracket depth
    let mut at_start = true;
    let mut blocks: Vec<(Block, usize, usize)> = Vec::new();
    while i < toks.len() {
        let t = &toks[i];
        match &t.tok {
            Tok::Newline | Tok::Sep(_) => {
                at_start = depth == 0;
                i += 1;
                continue;
            }
            Tok::Op("(") | Tok::Op("[") | Tok::Op("{") => depth += 1,
            Tok::Op(")") | Tok::Op("]") | Tok::Op("}") => depth -= 1,
            Tok::Ident(s) if at_start && depth == 0 && stack.last().map(|b| b.0) == Some(Block::Classdef) => {
                let kind = match s.as_str() {
                    "properties" => Block::Properties,
                    "methods" => Block::Methods,
                    "events" | "enumeration" => Block::Skip,
                    _ => return Err(err(format!("Unexpected '{s}' in classdef body."), t.line)),
                };
                stack.push((kind, t.line));
                // Rest of the header line (attributes) is not code.
                let l = t.line;
                while i < toks.len() && toks[i].line == l && !matches!(toks[i].tok, Tok::Newline) {
                    i += 1;
                }
                at_start = true;
                continue;
            }
            Tok::Kw(Kw::End) if depth == 0 => {
                let (kind, start) = stack.pop().ok_or_else(|| err("Unexpected 'end'.", t.line))?;
                if matches!(kind, Block::Properties | Block::Methods) {
                    blocks.push((kind, start, t.line));
                }
                if kind == Block::Classdef {
                    break;
                }
            }
            Tok::Kw(Kw::If | Kw::For | Kw::Parfor | Kw::While | Kw::Switch | Kw::Try | Kw::Function) if depth == 0 => {
                // Inside properties blocks only `end` matters.
                if stack.last().map(|b| b.0) != Some(Block::Properties) {
                    stack.push((Block::Other, t.line));
                }
            }
            _ => {}
        }
        at_start = false;
        i += 1;
    }
    let mut props = Vec::new();
    let mut methods = Vec::new();
    for (kind, start, end) in blocks {
        let attrs = attrs_of(line_text(start));
        match kind {
            Block::Properties => {
                let constant = has_attr(&attrs, "Constant");
                let dependent = has_attr(&attrs, "Dependent");
                let hidden = has_attr(&attrs, "Hidden") || access_private(&attrs);
                let mut l = start + 1;
                while l < end {
                    let mut text = line_text(l).trim().to_string();
                    let first = l;
                    while text.ends_with("...") && l + 1 < end {
                        text.truncate(text.len() - 3);
                        l += 1;
                        text.push(' ');
                        text.push_str(line_text(l).trim());
                    }
                    l += 1;
                    let text = text.trim().trim_end_matches(';').trim();
                    if text.is_empty() {
                        continue;
                    }
                    let pname: String = text.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                    if pname.is_empty() {
                        return Err(err(format!("Invalid property definition '{text}'."), first));
                    }
                    let rest = &text[pname.len()..];
                    let default = match find_assign(rest) {
                        Some(k) => Some(crate::parser::parse_expr(rest[k + 1..].trim()).map_err(|e| err(e.message, first))?),
                        None => None,
                    };
                    props.push(PropDef { name: pname, default, constant, dependent, hidden });
                }
            }
            Block::Methods => {
                if has_attr(&attrs, "Abstract") {
                    continue;
                }
                let is_static = has_attr(&attrs, "Static");
                let body: String = (start + 1..end).map(|l| *lines.get(l - 1).unwrap_or(&"")).collect::<Vec<_>>().join("\n");
                let prog = crate::parser::parse(&body).map_err(|e| err(e.message, e.line + start))?;
                for f in prog.functions {
                    methods.push(MethodDef { func: f, is_static });
                }
            }
            _ => {}
        }
    }
    Ok(ClassAst { name, supers, props, methods })
}

/// Position of a top-level `=` that is not part of `==`, `<=`, `>=`, `~=`.
fn find_assign(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    for i in 0..b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' if depth == 0 => {
                let prev = if i > 0 { b[i - 1] } else { b' ' };
                let next = b.get(i + 1).copied().unwrap_or(b' ');
                if next != b'=' && !matches!(prev, b'=' | b'<' | b'>' | b'~') {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_class_file() {
        let src = "classdef Account < handle\n  % A bank account\n  properties\n    Owner = 'nobody'\n    Balance (1,1) double = 0  % validation ignored\n  end\n  properties (Constant)\n    Rate = 0.05;\n  end\n  methods\n    function obj = Account(owner)\n      if nargin > 0\n        obj.Owner = owner;\n      end\n    end\n    function deposit(obj, x)\n      obj.Balance = obj.Balance + x;\n    end\n  end\n  methods (Static)\n    function a = make()\n      a = Account('x');\n    end\n  end\nend\n";
        let c = parse_classdef(src).unwrap();
        assert_eq!(c.name, "Account");
        assert_eq!(c.supers, vec!["handle".to_string()]);
        assert_eq!(c.props.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Owner", "Balance", "Rate"]);
        assert!(c.props[2].constant);
        assert_eq!(c.methods.len(), 3);
        assert!(c.methods.iter().any(|m| m.func.name == "make" && m.is_static));
    }
}
