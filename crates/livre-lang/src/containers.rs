//! `containers.Map`, `dictionary` and `categorical`.
//!
//! Each is an object of a generated class (as in `polyfun_classes`): methods are one-line MATLAB
//! functions forwarding to the hidden builtin `matlivre_container_method`, so function-call and
//! operator dispatch work as for user classes. Indexing, indexed assignment, concatenation and
//! display are native hooks called by the interpreter (`m('key')`, `d("k") = v`, `c(2) = 'x'`,
//! `[c1 c2]`), since map keys are not ordinary subscripts.
//!
//! - `containers.Map` is a handle: `Keys_`/`Values_` hold cell rows sorted by key.
//! - `dictionary` is a value class: `Keys_`/`Values_` are cell columns in insertion order, each
//!   element one key or value (a string, a number, or a 1×1 cell).
//! - `categorical` is a value class: `Codes` (0 = undefined) shaped as the array, `Cats` the
//!   category names.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::builtins::{num2str_scalar, one, values_equal, Ret};
use crate::class::{ClassDef, Object};
use crate::error::{rt, LangError};
use crate::display::NumFormat;
use crate::indexing;
use crate::interp::{Interpreter, OutputKind};
use crate::nd::{self, Dims, IndexArg};
use crate::value::*;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "containers.Map", "dictionary", "configureDictionary", "categorical", "iscategorical", "keys", "values", "isKey", "remove", "numEntries", "isConfigured",
    "types", "entries", "insert", "lookup", "categories", "countcats", "addcats", "removecats", "renamecats", "mergecats", "reordercats", "setcats", "isordinal",
    "isundefined", "iscategory", "isprotected", "matlivre_container_method", "double.empty", "single.empty", "logical.empty", "char.empty", "string.empty",
    "cell.empty", "struct.empty", "int8.empty", "int16.empty", "int32.empty", "int64.empty", "uint8.empty", "uint16.empty", "uint32.empty", "uint64.empty",
    "categorical.empty",
];

/// `T.empty` / `T.empty(m, n, ...)`: an empty array of a built-in class (0×0 by default).
fn empty_of(class: &str, a: &[Value]) -> Ret {
    let dims = if a.is_empty() { vec![0, 0] } else { crate::builtins::size_args(a)? };
    if nd::numel(&dims) != 0 {
        return rt("At least one dimension must be zero.");
    }
    one(match class {
        "string" => Value::Str(StrArray::from_dims(dims, Vec::new())),
        "cell" => Value::Cell(CellArray::from_dims(dims, Vec::new())),
        "struct" => Value::Struct(StructArray { dims, ..StructArray::empty_with(Vec::new()) }),
        "categorical" => Cat { codes: Matrix::from_dims(dims, Vec::new()), cats: Vec::new(), ordinal: false, protected: false }.value(),
        k => Value::Num(Matrix::from_dims(dims, Vec::new()).with_kind(Kind::from_name(k).unwrap_or(Kind::Double))),
    })
}

const MAP: &str = "containers.Map";
const DICT: &str = "dictionary";
const CAT: &str = "categorical";

const MAP_METHODS: &[&str] = &["keys", "values", "isKey", "remove", "length", "isempty"];
const DICT_METHODS: &[&str] = &["keys", "values", "isKey", "remove", "numEntries", "isConfigured", "types", "entries", "insert", "lookup", "isequal"];
const CAT_METHODS: &[&str] = &[
    "categories", "countcats", "addcats", "removecats", "renamecats", "mergecats", "reordercats", "setcats", "isordinal", "isundefined", "iscategory",
    "isprotected", "ismissing", "eq", "ne", "lt", "gt", "le", "ge", "transpose", "ctranspose", "double", "single", "string", "cellstr", "char", "unique",
    "sort", "ismember", "summary", "findgroups", "max", "min", "isequal", "numel", "length", "isempty", "horzcat", "vertcat",
];

/// Whether `name` is one of these classes.
pub fn is_container(name: &str) -> bool {
    matches!(name, MAP | DICT | CAT)
}

/// A map or dictionary: indexed by keys rather than subscripts.
pub fn is_keyed(v: &Value) -> bool {
    matches!(v, Value::Object(o) if o.class.name == MAP || o.class.name == DICT)
}

pub fn is_categorical(v: &Value) -> bool {
    matches!(v, Value::Object(o) if o.class.name == CAT)
}

fn class_def(class: &str) -> Rc<ClassDef> {
    thread_local! {
        static DEFS: RefCell<HashMap<String, Rc<ClassDef>>> = RefCell::new(HashMap::new());
    }
    DEFS.with(|defs| {
        if let Some(d) = defs.borrow().get(class) {
            return d.clone();
        }
        let (short, handle, props, hidden, methods): (&str, bool, &[&str], &[&str], &[&str]) = match class {
            MAP => ("Map", true, &["Count", "KeyType", "ValueType"], &["Keys_", "Values_"], MAP_METHODS),
            DICT => ("dictionary", false, &[], &["Keys_", "Values_", "KeyType_", "ValueType_"], DICT_METHODS),
            _ => ("categorical", false, &[], &["Codes", "Cats", "Ordinal", "Protected"], CAT_METHODS),
        };
        let mut src = format!("classdef {short}{}\n", if handle { " < handle" } else { "" });
        if !props.is_empty() {
            src.push_str("  properties\n");
            for p in props {
                src.push_str(&format!("    {p}\n"));
            }
            src.push_str("  end\n");
        }
        src.push_str("  properties (Hidden)\n");
        for p in hidden {
            src.push_str(&format!("    {p}\n"));
        }
        src.push_str("  end\n  methods\n");
        for m in methods {
            src.push_str(&format!(
                "    function varargout = {m}(varargin)\n      varargout = matlivre_container_method('{m}', nargout, varargin{{:}});\n    end\n"
            ));
        }
        src.push_str("  end\nend\n");
        let ast = crate::class::parse_classdef(&src).expect("built-in class source parses");
        let mut def = ClassDef::build(ast, &[]);
        def.name = class.to_string();
        let def = Rc::new(def);
        defs.borrow_mut().insert(class.to_string(), def.clone());
        def
    })
}

fn make(class: &str, values: Vec<Value>) -> Value {
    Value::Object(Object { class: class_def(class), props: Rc::new(RefCell::new(values)) })
}

fn prop(o: &Object, name: &str) -> Value {
    o.get(name).unwrap_or_else(Value::empty)
}

fn cell_items(v: Value) -> Vec<Value> {
    match v {
        Value::Cell(c) => c.data,
        _ => Vec::new(),
    }
}

/// The elements of an array as scalar values (a cell gives 1×1 cells).
fn elements(v: &Value) -> Vec<Value> {
    let n = v.numel();
    (0..n).map(|i| indexing::index(v, &[IndexArg::scalar(i)]).unwrap_or_else(|_| v.clone())).collect()
}

fn reshape(v: Value, dims: Dims) -> Value {
    let dims = nd::normalize(dims);
    match v {
        Value::Num(m) => Value::Num(m.reshaped(dims)),
        Value::Cell(c) => Value::Cell(CellArray::from_dims(dims, c.data)),
        Value::Str(s) => Value::Str(StrArray::from_dims(dims, s.data)),
        Value::Struct(s) => Value::Struct(StructArray { dims, ..s }),
        other => other,
    }
}

/// Concatenates scalar elements into an array of shape `dims`.
fn assemble(items: Vec<Value>, dims: Dims) -> R<Value> {
    if items.is_empty() {
        return Ok(Value::Num(Matrix::from_dims(dims, Vec::new())));
    }
    Ok(reshape(indexing::vconcat(items)?, dims))
}

fn is_text(v: &Value) -> bool {
    matches!(v, Value::Num(m) if m.kind == Kind::Char) || matches!(v, Value::Str(_))
}

/// Text items: a char row, a char matrix (one item per row), a string array or a cellstr.
/// Missing strings are `None`.
fn text_items(v: &Value) -> R<Vec<Option<String>>> {
    match v {
        Value::Num(m) if m.kind == Kind::Char => {
            if m.rows() <= 1 {
                return Ok(vec![Some(m.to_string_lossy())]);
            }
            Ok((0..m.rows()).map(|r| Some((0..m.cols()).map(|c| char::from_u32(m.get(r, c) as u32).unwrap_or('?')).collect::<String>().trim_end().to_string())).collect())
        }
        Value::Str(s) => Ok(s.data.clone()),
        Value::Cell(c) => c.data.iter().map(|x| x.text().map(Some)).collect(),
        Value::Num(m) if m.is_empty() => Ok(Vec::new()),
        other => rt(format!("Expected text but got a value of class '{}'.", other.class_name())),
    }
}

fn texts(v: &Value) -> R<Vec<String>> {
    Ok(text_items(v)?.into_iter().map(|t| t.unwrap_or_default()).collect())
}

fn logical(dims: Dims, data: Vec<bool>) -> Value {
    Value::Num(Matrix::from_dims(dims, data.into_iter().map(|b| if b { 1.0 } else { 0.0 }).collect()).with_kind(Kind::Logical))
}

fn find_obj<'a>(a: &'a [Value], class: &str) -> Option<&'a Object> {
    a.iter().find_map(|v| match v {
        Value::Object(o) if o.class.name == class => Some(o),
        _ => None,
    })
}

// ---- containers.Map ------------------------------------------------------------------------

fn numeric_key_type(kind: Kind) -> &'static str {
    match kind.name() {
        k @ ("single" | "int32" | "uint32" | "int64" | "uint64") => k,
        _ => "double",
    }
}

/// The value type inferred from the values given to the constructor.
fn infer_value_type(vals: &[Value]) -> String {
    let Some(first) = vals.first() else { return "any".into() };
    match first {
        Value::Num(m) if m.kind == Kind::Char && vals.iter().all(|v| matches!(v, Value::Num(x) if x.kind == Kind::Char && x.rows() <= 1)) => "char".into(),
        Value::Num(m) if m.kind != Kind::Char && vals.iter().all(|v| matches!(v, Value::Num(x) if x.kind == m.kind && x.numel() == 1 && !x.is_complex())) => m.kind.name().into(),
        _ => "any".into(),
    }
}

fn map_parts(o: &Object) -> (Vec<Value>, Vec<Value>, String, String) {
    (cell_items(prop(o, "Keys_")), cell_items(prop(o, "Values_")), prop(o, "KeyType").text().unwrap_or_default(), prop(o, "ValueType").text().unwrap_or_default())
}

fn map_store(o: &Object, keys: Vec<Value>, vals: Vec<Value>) {
    let mut p = o.props.borrow_mut();
    let count = Matrix::scalar(keys.len() as f64).with_kind(Kind::from_name("uint64").unwrap());
    let ix = |n: &str| o.class.prop_index(n).unwrap();
    p[ix("Count")] = Value::Num(count);
    p[ix("Keys_")] = Value::Cell(CellArray::row(keys));
    p[ix("Values_")] = Value::Cell(CellArray::row(vals));
}

/// A key checked against the key type: text for char maps, a real scalar for numeric ones.
fn map_key(key: &Value, key_type: &str) -> R<Value> {
    let bad = || rt("Specified key type does not match the type expected for this container.");
    if key_type == "char" {
        return match key {
            Value::Num(m) if m.kind == Kind::Char && m.rows() <= 1 => Ok(Value::chars(&m.to_string_lossy())),
            Value::Str(s) if s.data.len() == 1 && s.data[0].is_some() => Ok(Value::chars(s.data[0].as_deref().unwrap())),
            _ => bad(),
        };
    }
    match key {
        Value::Num(m) if m.kind != Kind::Char && m.numel() == 1 && !m.is_complex() => Ok(Value::num(m.data[0])),
        _ => bad(),
    }
}

fn key_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) if x.kind == Kind::Char && y.kind == Kind::Char => x.to_string_lossy().cmp(&y.to_string_lossy()),
        (Value::Num(x), Value::Num(y)) => x.data[0].partial_cmp(&y.data[0]).unwrap_or(std::cmp::Ordering::Equal),
        _ => std::cmp::Ordering::Equal,
    }
}

fn map_value(v: Value, value_type: &str) -> R<Value> {
    let bad = || rt("Specified value type does not match the type expected for this container.");
    match value_type {
        "any" => Ok(v),
        "char" => match &v {
            Value::Num(m) if m.kind == Kind::Char || m.is_empty() => Ok(v),
            Value::Str(s) if s.data.len() == 1 => Ok(Value::chars(s.data[0].as_deref().unwrap_or(""))),
            _ => bad(),
        },
        t => match (&v, Kind::from_name(t)) {
            (Value::Num(m), Some(k)) if m.kind != Kind::Char && m.numel() == 1 => {
                let x = if k == Kind::Logical { (m.data[0] != 0.0) as u8 as f64 } else { k.cast(m.data[0]) };
                Ok(Value::Num(Matrix::scalar(x).with_kind(k)))
            }
            _ => bad(),
        },
    }
}

fn new_map(a: &[Value]) -> Ret {
    let is_opt = |v: &Value| v.text().map(|t| ["keytype", "valuetype", "uniformvalues"].contains(&t.to_lowercase().as_str())).unwrap_or(false);
    let (keys_arg, vals_arg, opts) = if a.is_empty() || (is_opt(&a[0]) && a.len() % 2 == 0) { (None, None, a) } else if a.len() >= 2 { (Some(&a[0]), Some(&a[1]), &a[2..]) } else { return rt("Not enough input arguments.") };
    let mut key_type = String::from("char");
    let mut value_type: Option<String> = None;
    let mut uniform = true;
    for p in opts.chunks(2) {
        let [n, v] = p else { return rt("Invalid parameter/value pair arguments.") };
        match n.text()?.to_lowercase().as_str() {
            "keytype" => {
                let t = v.text()?;
                if !["char", "double", "single", "int32", "uint32", "int64", "uint64"].contains(&t.as_str()) {
                    return rt(format!("Unsupported key type '{t}'."));
                }
                key_type = t;
            }
            "valuetype" => value_type = Some(v.text()?),
            "uniformvalues" => uniform = v.is_true()?,
            other => return rt(format!("Unknown parameter '{other}'.")),
        }
    }
    let mut keys: Vec<Value> = Vec::new();
    let mut vals: Vec<Value> = Vec::new();
    if let (Some(k), Some(v)) = (keys_arg, vals_arg) {
        keys = match k {
            Value::Num(m) if m.kind == Kind::Char => vec![k.clone()],
            Value::Str(_) => texts(k)?.into_iter().map(|t| Value::chars(&t)).collect(),
            Value::Cell(c) => c.data.clone(),
            Value::Num(m) => {
                key_type = numeric_key_type(m.kind).into();
                elements(k)
            }
            _ => return rt("Specified key type does not match the type expected for this container."),
        };
        if let Some(Value::Num(m)) = keys.first() {
            if m.kind != Kind::Char {
                key_type = numeric_key_type(m.kind).into();
            }
        }
        vals = match v {
            Value::Cell(c) if keys.len() != 1 || c.data.len() == 1 => c.data.clone(),
            _ if keys.len() == 1 => vec![v.clone()],
            _ => elements(v),
        };
        if keys.len() != vals.len() {
            return rt("The number of keys and values must be the same.");
        }
    }
    let value_type = value_type.unwrap_or_else(|| if uniform { infer_value_type(&vals) } else { "any".into() });
    let mut pairs: Vec<(Value, Value)> = Vec::new();
    for (k, v) in keys.into_iter().zip(vals) {
        let k = map_key(&k, &key_type)?;
        let v = map_value(v, &value_type)?;
        match pairs.iter_mut().find(|(x, _)| values_equal(x, &k)) {
            Some(p) => p.1 = v,
            None => pairs.push((k, v)),
        }
    }
    pairs.sort_by(|x, y| key_cmp(&x.0, &y.0));
    let (k, v): (Vec<Value>, Vec<Value>) = pairs.into_iter().unzip();
    let obj = make(MAP, vec![Value::empty(), Value::chars(&key_type), Value::chars(&value_type), Value::empty(), Value::empty()]);
    if let Value::Object(o) = &obj {
        map_store(o, k, v);
    }
    one(obj)
}

fn map_get(o: &Object, key: &Value) -> R<Value> {
    let (keys, vals, kt, _) = map_parts(o);
    let k = map_key(key, &kt)?;
    match keys.iter().position(|x| values_equal(x, &k)) {
        Some(i) => Ok(vals[i].clone()),
        None => rt("The specified key is not present in this container."),
    }
}

fn map_set(o: &Object, key: &Value, v: Value) -> R<()> {
    let (mut keys, mut vals, kt, vt) = map_parts(o);
    let k = map_key(key, &kt)?;
    let v = map_value(v, &vt)?;
    match keys.iter().position(|x| values_equal(x, &k)) {
        Some(i) => vals[i] = v,
        None => {
            let at = keys.iter().position(|x| key_cmp(x, &k) == std::cmp::Ordering::Greater).unwrap_or(keys.len());
            keys.insert(at, k);
            vals.insert(at, v);
        }
    }
    map_store(o, keys, vals);
    Ok(())
}

/// Keys given as one key or a cell of keys, with the shape of the result.
fn key_list(v: &Value) -> (Vec<Value>, Dims, bool) {
    match v {
        Value::Cell(c) => (c.data.clone(), c.dims.clone(), true),
        Value::Str(s) if s.data.len() != 1 => (elements(v), s.dims.clone(), true),
        _ => (vec![v.clone()], vec![1, 1], false),
    }
}

fn map_method(it: &mut Interpreter, m: &str, o: &Object, a: &[Value]) -> Ret {
    let (keys, vals, _, _) = map_parts(o);
    match m {
        "keys" => one(Value::Cell(CellArray::from_dims(vec![1, keys.len()], keys))),
        "values" => match a.get(1) {
            None => one(Value::Cell(CellArray::from_dims(vec![1, vals.len()], vals))),
            Some(Value::Cell(c)) => {
                let out = c.data.iter().map(|k| map_get(o, k)).collect::<R<Vec<_>>>()?;
                one(Value::Cell(CellArray::from_dims(c.dims.clone(), out)))
            }
            Some(_) => rt("Specified key type does not match the type expected for this container."),
        },
        "isKey" => {
            let (ks, dims, _) = key_list(a.get(1).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?);
            let kt = prop(o, "KeyType").text()?;
            let found: Vec<bool> = ks.iter().map(|k| map_key(k, &kt).map(|k| keys.iter().any(|x| values_equal(x, &k))).unwrap_or(false)).collect();
            one(logical(dims, found))
        }
        "remove" => {
            let (ks, _, _) = key_list(a.get(1).ok_or_else(|| LangError::new("Not enough input arguments.", 0))?);
            let kt = prop(o, "KeyType").text()?;
            let (mut keys, mut vals) = (keys, vals);
            for k in &ks {
                let k = map_key(k, &kt)?;
                match keys.iter().position(|x| values_equal(x, &k)) {
                    Some(i) => {
                        keys.remove(i);
                        vals.remove(i);
                    }
                    None => it.emit(OutputKind::Warning("The specified key is not present in this container.".into())),
                }
            }
            map_store(o, keys, vals);
            one(Value::Object(o.clone()))
        }
        "length" => one(Value::num(keys.len() as f64)),
        "isempty" => one(Value::bool(keys.is_empty())),
        _ => rt(format!("Undefined function '{m}' for input arguments of type 'containers.Map'.")),
    }
}

fn map_display(name: &str, o: &Object) -> String {
    let (keys, _, kt, vt) = map_parts(o);
    format!("{name} = \n  Map with properties:\n\n        Count: {}\n      KeyType: {kt}\n    ValueType: {vt}", keys.len())
}

// ---- dictionary ----------------------------------------------------------------------------

struct Dict {
    keys: Vec<Value>,
    vals: Vec<Value>,
    kt: String,
    vt: String,
}

impl Dict {
    fn of(o: &Object) -> Dict {
        Dict { keys: cell_items(prop(o, "Keys_")), vals: cell_items(prop(o, "Values_")), kt: prop(o, "KeyType_").text().unwrap_or_default(), vt: prop(o, "ValueType_").text().unwrap_or_default() }
    }

    fn value(self) -> Value {
        make(DICT, vec![Value::Cell(CellArray::col(self.keys)), Value::Cell(CellArray::col(self.vals)), Value::chars(&self.kt), Value::chars(&self.vt)])
    }

    fn configured(&self) -> bool {
        !self.kt.is_empty()
    }

    /// The keys of `v` converted to the key type (char becomes string; numbers become strings in
    /// a string dictionary).
    fn key_elements(&self, v: &Value) -> R<Vec<Value>> {
        let kt = if self.configured() { self.kt.clone() } else { type_of(v) };
        Ok(match (kt.as_str(), v) {
            ("string", Value::Num(m)) if m.kind == Kind::Char => vec![Value::string(m.to_string_lossy())],
            ("string", Value::Num(m)) if m.kind != Kind::Char => m.data.iter().map(|&x| Value::string(num2str_scalar(x))).collect(),
            ("string", Value::Cell(_)) => texts(v)?.into_iter().map(Value::string).collect(),
            (_, Value::Num(m)) if m.kind == Kind::Char => vec![v.clone()],
            _ => elements(v),
        })
    }

    fn find(&self, k: &Value) -> Option<usize> {
        self.keys.iter().position(|x| values_equal(x, k))
    }

    fn lookup(&self, key: &Value) -> R<Value> {
        if !self.configured() {
            return rt("Key not found.");
        }
        let ks = self.key_elements(key)?;
        let mut out = Vec::with_capacity(ks.len());
        for (i, k) in ks.iter().enumerate() {
            match self.find(k) {
                Some(p) => out.push(self.vals[p].clone()),
                None if ks.len() == 1 => return rt("Key not found."),
                None => return rt(format!("Element {} of the key array not found.", i + 1)),
            }
        }
        if out.len() == 1 {
            return Ok(out.pop().unwrap());
        }
        let dims = if matches!(key, Value::Num(m) if m.kind == Kind::Char) { vec![1, 1] } else { key.dims() };
        assemble(out, dims)
    }

    fn insert(&mut self, key: &Value, v: Value) -> R<()> {
        if !self.configured() {
            self.kt = type_of(key);
            self.vt = type_of(&v);
        }
        let ks = self.key_elements(key)?;
        // d(k) = [] removes the entries.
        if matches!(&v, Value::Num(m) if m.dims == [0, 0] && m.kind == Kind::Double) {
            for k in &ks {
                if let Some(p) = self.find(k) {
                    self.keys.remove(p);
                    self.vals.remove(p);
                }
            }
            return Ok(());
        }
        let vs = self.value_elements(&v)?;
        if vs.len() != ks.len() && vs.len() != 1 {
            return rt("Dimensions of the key and value must be compatible, or the value must be scalar.");
        }
        for (i, k) in ks.into_iter().enumerate() {
            let val = vs[if vs.len() == 1 { 0 } else { i }].clone();
            match self.find(&k) {
                Some(p) => self.vals[p] = val,
                None => {
                    self.keys.push(k);
                    self.vals.push(val);
                }
            }
        }
        Ok(())
    }

    fn value_elements(&self, v: &Value) -> R<Vec<Value>> {
        Ok(match (self.vt.as_str(), v) {
            ("string", Value::Num(m)) if m.kind == Kind::Char => vec![Value::string(m.to_string_lossy())],
            ("cell", Value::Cell(_)) => elements(v),
            ("cell", _) => vec![Value::Cell(CellArray::new(1, 1, vec![v.clone()]))],
            (t, Value::Num(m)) if m.kind != Kind::Char && Kind::from_name(t).is_some() && Kind::from_name(t) != Some(m.kind) => {
                let k = Kind::from_name(t).unwrap();
                elements(&Value::Num(Matrix::from_dims(m.dims.clone(), m.data.iter().map(|&x| k.cast(x)).collect()).with_kind(k)))
            }
            _ => elements(v),
        })
    }
}

/// The dictionary type name of a key or value array.
fn type_of(v: &Value) -> String {
    match v {
        Value::Num(m) if m.kind == Kind::Char => "string".into(),
        Value::Num(m) => m.kind.name().into(),
        Value::Str(_) => "string".into(),
        other => other.class_name(),
    }
}

fn new_dict(a: &[Value]) -> Ret {
    let mut d = Dict { keys: Vec::new(), vals: Vec::new(), kt: String::new(), vt: String::new() };
    if a.len() % 2 != 0 {
        return rt("Keys and values must be specified in pairs.");
    }
    for p in a.chunks(2) {
        if !d.configured() {
            d.kt = type_of(&p[0]);
            d.vt = type_of(&p[1]);
        }
        let n = if matches!(&p[0], Value::Num(m) if m.kind == Kind::Char) { 1 } else { p[0].numel() };
        if p[1].numel() != n && p[1].numel() != 1 && !(d.vt == "cell" && n == 1) {
            return rt("Dimensions of the key and value must be compatible, or the value must be scalar.");
        }
        if n > 0 {
            d.insert(&p[0], p[1].clone())?;
        }
    }
    one(d.value())
}

fn configure_dict(a: &[Value]) -> Ret {
    let kt = crate::builtins::arg(a, 0, "configureDictionary")?.text()?;
    let vt = crate::builtins::arg(a, 1, "configureDictionary")?.text()?;
    one(Dict { keys: Vec::new(), vals: Vec::new(), kt, vt }.value())
}

fn dict_method(m: &str, o: &Object, a: &[Value]) -> Ret {
    let mut d = Dict::of(o);
    let as_cell = a.last().and_then(|v| v.text().ok()).map(|t| t == "cell").unwrap_or(false) && a.len() >= 2 && (d.kt != "string" || a.len() == 3 || m == "keys");
    let column = |items: Vec<Value>, cell: bool| -> R<Value> {
        let n = items.len();
        if cell {
            return Ok(Value::Cell(CellArray::from_dims(vec![n, 1], items)));
        }
        if n == 0 {
            return Ok(match d.vt.as_str() {
                "string" => Value::Str(StrArray::from_dims(vec![0, 1], Vec::new())),
                _ => Value::Num(Matrix::from_dims(vec![0, 1], Vec::new())),
            });
        }
        assemble(items, vec![n, 1])
    };
    match m {
        "keys" => {
            let items = d.keys.clone();
            if as_cell {
                return one(Value::Cell(CellArray::from_dims(vec![d.keys.len(), 1], items)));
            }
            if items.is_empty() {
                return one(match d.kt.as_str() {
                    "string" => Value::Str(StrArray::from_dims(vec![0, 1], Vec::new())),
                    _ => Value::Num(Matrix::from_dims(vec![0, 1], Vec::new())),
                });
            }
            one(assemble(items, vec![d.keys.len(), 1])?)
        }
        "values" => {
            if let Some(k) = a.get(1).filter(|_| !(as_cell && a.len() == 2)) {
                return one(d.lookup(k)?);
            }
            let cell = as_cell;
            one(column(d.vals.clone(), cell)?)
        }
        "lookup" => {
            let k = crate::builtins::arg(a, 1, "lookup")?;
            match d.lookup(k) {
                Err(_) if a.len() >= 4 && a[2].text().map(|t| t.eq_ignore_ascii_case("FallbackValue")).unwrap_or(false) => {
                    let ks = d.key_elements(k)?;
                    let out: Vec<Value> = ks.iter().map(|x| d.find(x).map(|p| d.vals[p].clone()).unwrap_or_else(|| a[3].clone())).collect();
                    if out.len() == 1 {
                        return one(out.into_iter().next().unwrap());
                    }
                    one(assemble(out, k.dims())?)
                }
                r => one(r?),
            }
        }
        "isKey" => {
            let k = crate::builtins::arg(a, 1, "isKey")?;
            let ks = if d.configured() { d.key_elements(k)? } else { elements(k) };
            let dims = if is_text(k) && matches!(k, Value::Num(_)) { vec![1, 1] } else { k.dims() };
            one(logical(dims, ks.iter().map(|x| d.find(x).is_some()).collect()))
        }
        "remove" => {
            let k = crate::builtins::arg(a, 1, "remove")?;
            for x in d.key_elements(k)? {
                match d.find(&x) {
                    Some(p) => {
                        d.keys.remove(p);
                        d.vals.remove(p);
                    }
                    None => return rt("Key not found."),
                }
            }
            one(d.value())
        }
        "insert" => {
            let k = crate::builtins::arg(a, 1, "insert")?;
            let v = crate::builtins::arg(a, 2, "insert")?;
            d.insert(k, v.clone())?;
            one(d.value())
        }
        "numEntries" => one(Value::num(d.keys.len() as f64)),
        "isConfigured" => one(Value::bool(d.configured())),
        "types" => Ok(Some(vec![Value::string(d.kt.clone()), Value::string(d.vt.clone())])),
        "entries" => {
            let fmt = a.get(1).and_then(|v| v.text().ok()).unwrap_or_else(|| "table".into());
            match fmt.as_str() {
                "struct" => {
                    let mut s = StructArray::empty_with(vec!["Key".into(), "Value".into()]);
                    s.dims = vec![d.keys.len(), 1];
                    s.elems = d.keys.iter().zip(&d.vals).map(|(k, v)| vec![k.clone(), v.clone()]).collect();
                    one(Value::Struct(s))
                }
                "cell" => {
                    let n = d.keys.len();
                    let data: Vec<Value> = d.keys.iter().chain(d.vals.iter()).cloned().collect();
                    one(Value::Cell(CellArray::from_dims(vec![n, 2], data)))
                }
                _ => {
                    let n = d.keys.len();
                    let k = column(d.keys.clone(), false)?;
                    let v = column(d.vals.clone(), false)?;
                    let _ = n;
                    one(Value::Table(Box::new(crate::table::Table::new(vec!["Key".into(), "Value".into()], vec![k, v])?)))
                }
            }
        }
        "isequal" => {
            let all = a.iter().all(|v| match v {
                Value::Object(x) if x.class.name == DICT => {
                    let e = Dict::of(x);
                    e.kt == d.kt && e.vt == d.vt && e.keys.len() == d.keys.len() && e.keys.iter().zip(&e.vals).all(|(k, v)| d.find(k).map(|p| values_equal(&d.vals[p], v)).unwrap_or(false))
                }
                _ => false,
            });
            one(Value::bool(all))
        }
        _ => rt(format!("Undefined function '{m}' for input arguments of type 'dictionary'.")),
    }
}

fn entry_text(v: &Value, f: NumFormat) -> String {
    match v {
        Value::Str(s) if s.data.len() == 1 => match &s.data[0] {
            Some(t) => format!("\"{t}\""),
            None => "<missing>".into(),
        },
        Value::Cell(c) if c.data.len() == 1 => crate::display::disp_value(v, f).trim().to_string(),
        Value::Num(m) if m.kind != Kind::Char && m.numel() == 1 => crate::display::fmt_elem(m, 0, f).trim().to_string(),
        other => crate::display::summary(other, f),
    }
}

fn dict_display(name: &str, o: &Object, f: NumFormat) -> String {
    let d = Dict::of(o);
    if !d.configured() {
        return format!("{name} = \n  dictionary with unset key and value types.");
    }
    let head = format!("{name} = \n  dictionary ({} --> {})", d.kt, d.vt);
    match d.keys.len() {
        0 => format!("{head} with no entries."),
        n => {
            let mut out = format!("{head} with {n} {}:\n", if n == 1 { "entry" } else { "entries" });
            for (k, v) in d.keys.iter().zip(&d.vals) {
                out.push_str(&format!("\n    {} --> {}", entry_text(k, f), entry_text(v, f)));
            }
            out
        }
    }
}

// ---- categorical ---------------------------------------------------------------------------

#[derive(Clone)]
struct Cat {
    /// 1-based category of each element; 0 is undefined.
    codes: Matrix,
    cats: Vec<String>,
    ordinal: bool,
    protected: bool,
}

const UNDEF: &str = "<undefined>";

impl Cat {
    fn of(o: &Object) -> Cat {
        let codes = prop(o, "Codes").to_matrix().unwrap_or_else(|_| Matrix::empty());
        let cats = texts(&prop(o, "Cats")).unwrap_or_default();
        Cat { codes, cats, ordinal: prop(o, "Ordinal").is_true().unwrap_or(false), protected: prop(o, "Protected").is_true().unwrap_or(false) }
    }

    fn from_value(v: &Value) -> Option<Cat> {
        match v {
            Value::Object(o) if o.class.name == CAT => Some(Cat::of(o)),
            _ => None,
        }
    }

    fn value(self) -> Value {
        let n = self.cats.len();
        let cats = Value::Cell(CellArray::from_dims(vec![n, 1], self.cats.iter().map(|c| Value::chars(c)).collect()));
        make(CAT, vec![Value::Num(self.codes), cats, Value::bool(self.ordinal), Value::bool(self.protected || self.ordinal)])
    }

    fn with_codes(&self, codes: Matrix) -> Cat {
        Cat { codes, ..self.clone() }
    }

    fn name(&self, i: usize) -> Option<&str> {
        let c = self.codes.data[i] as usize;
        if c == 0 { None } else { self.cats.get(c - 1).map(|s| s.as_str()) }
    }

    fn names(&self) -> Vec<Option<String>> {
        (0..self.codes.data.len()).map(|i| self.name(i).map(|s| s.to_string())).collect()
    }

    fn dims(&self) -> Dims {
        self.codes.dims.clone()
    }

    fn code_of(&self, name: &str) -> Option<usize> {
        self.cats.iter().position(|c| c == name).map(|p| p + 1)
    }

    /// Codes for `v` (text or categorical) in this array's categories, adding new categories
    /// unless they are protected.
    fn codes_for(&mut self, v: &Value) -> R<Matrix> {
        let (names, dims) = match Cat::from_value(v) {
            Some(c) => (c.names(), c.dims()),
            None if matches!(v, Value::Num(m) if m.kind == Kind::Char && m.rows() <= 1) => (vec![Some(v.text()?).filter(|t| !t.is_empty())], vec![1, 1]),
            None => (text_items(v)?.into_iter().map(|t| t.filter(|t| !t.is_empty())).collect(), v.dims()),
        };
        let mut data = Vec::with_capacity(names.len());
        for n in names {
            data.push(match n {
                None => 0.0,
                Some(n) => match self.code_of(&n) {
                    Some(c) => c as f64,
                    None if self.protected || self.ordinal => {
                        return rt(format!("Cannot add a new category '{n}' to this categorical array because its categories are protected.  Use ADDCATS to add the new category."))
                    }
                    None => {
                        self.cats.push(n);
                        self.cats.len() as f64
                    }
                },
            });
        }
        Ok(Matrix::from_dims(dims, data))
    }
}

/// Values of numeric or text data as category keys.
enum Data {
    Text(Vec<Option<String>>),
    Num(Vec<f64>),
}

fn data_of(v: &Value) -> R<(Data, Dims)> {
    match v {
        Value::Num(m) if m.kind != Kind::Char => Ok((Data::Num(m.data.clone()), m.dims.clone())),
        Value::Num(m) if m.rows() <= 1 => Ok((Data::Text(vec![Some(m.to_string_lossy())]), vec![1, 1])),
        Value::Num(m) => Ok((Data::Text(text_items(v)?), vec![m.rows(), 1])),
        _ => Ok((Data::Text(text_items(v)?), v.dims())),
    }
}

fn new_categorical(a: &[Value]) -> Ret {
    let Some(data) = a.first() else { return one(Cat { codes: Matrix::empty(), cats: Vec::new(), ordinal: false, protected: false }.value()) };
    let mut pos = Vec::new();
    let mut ordinal = false;
    let mut protected = false;
    let mut i = 1;
    while i < a.len() {
        let opt = a[i].text().ok().map(|t| t.to_lowercase());
        match opt.as_deref() {
            Some("ordinal") if i + 1 < a.len() => {
                ordinal = a[i + 1].is_true()?;
                i += 2;
            }
            Some("protected") if i + 1 < a.len() => {
                protected = a[i + 1].is_true()?;
                i += 2;
            }
            _ => {
                pos.push(&a[i]);
                i += 1;
            }
        }
    }
    if let Some(c) = Cat::from_value(data) {
        if pos.is_empty() {
            return one(Cat { ordinal: ordinal || c.ordinal, protected: protected || c.protected, ..c }.value());
        }
        let names = c.names();
        let s = StrArray::from_dims(c.dims(), names);
        let mut rest: Vec<Value> = vec![Value::Str(s)];
        rest.extend(a[1..].iter().cloned());
        return new_categorical(&rest);
    }
    let (d, dims) = data_of(data)?;
    // The value set: given, or the sorted distinct values.
    let set: Data = match pos.first() {
        Some(v) => data_of(v)?.0,
        None => match &d {
            Data::Num(x) => {
                let mut u: Vec<f64> = x.iter().copied().filter(|v| !v.is_nan()).collect();
                u.sort_by(|p, q| p.partial_cmp(q).unwrap());
                u.dedup();
                Data::Num(u)
            }
            Data::Text(t) => {
                let mut u: Vec<String> = t.iter().flatten().filter(|s| !s.is_empty()).cloned().collect();
                u.sort();
                u.dedup();
                Data::Text(u.into_iter().map(Some).collect())
            }
        },
    };
    let set_names: Vec<String> = match &set {
        Data::Num(x) => x.iter().map(|&v| num2str_scalar(v)).collect(),
        Data::Text(t) => t.iter().map(|s| s.clone().unwrap_or_default()).collect(),
    };
    let cats = match pos.get(1) {
        Some(n) => {
            let n = texts(n)?;
            if n.len() != set_names.len() {
                return rt("Number of category names must match the number of categories.");
            }
            n
        }
        None => set_names,
    };
    let codes: Vec<f64> = match (&d, &set) {
        (Data::Num(x), Data::Num(s)) => x.iter().map(|v| s.iter().position(|w| w == v).map(|p| p as f64 + 1.0).unwrap_or(0.0)).collect(),
        (Data::Text(x), Data::Text(s)) => x.iter().map(|v| v.as_ref().and_then(|v| s.iter().position(|w| w.as_deref() == Some(v.as_str()))).map(|p| p as f64 + 1.0).unwrap_or(0.0)).collect(),
        (Data::Num(x), Data::Text(s)) => x.iter().map(|v| s.iter().position(|w| w.as_deref() == Some(num2str_scalar(*v).as_str())).map(|p| p as f64 + 1.0).unwrap_or(0.0)).collect(),
        (Data::Text(x), Data::Num(s)) => x.iter().map(|v| v.as_ref().and_then(|v| v.trim().parse::<f64>().ok()).and_then(|v| s.iter().position(|w| *w == v)).map(|p| p as f64 + 1.0).unwrap_or(0.0)).collect(),
    };
    let dims = if codes.is_empty() && data.numel() == 0 { data.dims() } else { dims };
    one(Cat { codes: Matrix::from_dims(dims, codes), cats, ordinal, protected: protected || ordinal }.value())
}

/// Element names of a comparison operand (categorical or text), with its dims.
fn operand_names(v: &Value) -> R<(Vec<Option<String>>, Dims)> {
    if let Some(c) = Cat::from_value(v) {
        return Ok((c.names(), c.dims()));
    }
    if matches!(v, Value::Num(m) if m.kind == Kind::Char && m.rows() <= 1) {
        return Ok((vec![Some(v.text()?)], vec![1, 1]));
    }
    if is_text(v) || matches!(v, Value::Cell(_)) {
        return Ok((text_items(v)?, v.dims()));
    }
    rt(format!("Invalid types for comparison: categorical and {}.", v.class_name()))
}

fn broadcast<T: Clone>(a: &[T], da: &Dims, b: &[T], db: &Dims) -> R<(Vec<(T, T)>, Dims)> {
    if a.len() == 1 {
        return Ok((b.iter().map(|y| (a[0].clone(), y.clone())).collect(), db.clone()));
    }
    if b.len() == 1 {
        return Ok((a.iter().map(|x| (x.clone(), b[0].clone())).collect(), da.clone()));
    }
    if da != db {
        return rt("Arrays have incompatible sizes for this operation.");
    }
    Ok((a.iter().cloned().zip(b.iter().cloned()).collect(), da.clone()))
}

fn compare(op: &str, a: &[Value]) -> Ret {
    let (l, r) = (crate::builtins::arg(a, 0, op)?, crate::builtins::arg(a, 1, op)?);
    if matches!(op, "eq" | "ne") {
        let (x, dx) = operand_names(l)?;
        let (y, dy) = operand_names(r)?;
        let (pairs, dims) = broadcast(&x, &dx, &y, &dy)?;
        let eq = op == "eq";
        return one(logical(dims, pairs.into_iter().map(|(p, q)| matches!((&p, &q), (Some(p), Some(q)) if p == q) == eq).collect()));
    }
    let c = Cat::from_value(l).or_else(|| Cat::from_value(r)).unwrap();
    if !c.ordinal {
        return rt("Relational comparisons are not allowed for non-ordinal categorical arrays.");
    }
    let codes = |v: &Value| -> R<(Vec<f64>, Dims)> {
        if let Some(d) = Cat::from_value(v) {
            if d.cats != c.cats || !d.ordinal {
                return rt("Relational comparisons are not allowed for ordinal categorical arrays with different categories.");
            }
            return Ok((d.codes.data.clone(), d.dims()));
        }
        let (names, dims) = operand_names(v)?;
        let out = names.iter().map(|n| n.as_deref().and_then(|n| c.code_of(n)).map(|x| x as f64).ok_or_else(|| LangError::new(format!("Invalid comparison: '{}' is not a category.", n.clone().unwrap_or_default()), 0))).collect::<R<Vec<_>>>()?;
        Ok((out, dims))
    };
    let (x, dx) = codes(l)?;
    let (y, dy) = codes(r)?;
    let (pairs, dims) = broadcast(&x, &dx, &y, &dy)?;
    let f = |p: f64, q: f64| {
        p != 0.0
            && q != 0.0
            && match op {
                "lt" => p < q,
                "gt" => p > q,
                "le" => p <= q,
                _ => p >= q,
            }
    };
    one(logical(dims, pairs.into_iter().map(|(p, q)| f(p, q)).collect()))
}

/// `[a b]` / `[a; b]` with categorical and text operands.
pub fn concat(items: Vec<Value>, dim: usize) -> R<Value> {
    let first = items.iter().find_map(Cat::from_value).unwrap();
    let mut out = Cat { codes: Matrix::empty(), ..first.clone() };
    let mut parts = Vec::with_capacity(items.len());
    for v in &items {
        if matches!(v, Value::Num(m) if m.is_empty() && m.kind != Kind::Char) {
            continue;
        }
        let ord = out.ordinal;
        out.ordinal = false;
        let saved = out.protected;
        if Cat::from_value(v).map(|c| !c.protected).unwrap_or(true) || !ord {
            out.protected = false;
        }
        parts.push(Value::Num(out.codes_for(v)?));
        out.ordinal = ord;
        out.protected = saved;
    }
    let codes = if dim == 1 { indexing::vconcat(parts)? } else { indexing::hconcat(parts)? };
    out.codes = codes.to_matrix()?;
    Ok(out.value())
}

/// `c(idx)`.
pub fn cat_index(v: &Value, idx: &[IndexArg]) -> R<Value> {
    let c = Cat::from_value(v).unwrap();
    let codes = indexing::index(&Value::Num(c.codes.clone()), idx)?.to_matrix()?;
    Ok(c.with_codes(codes).value())
}

/// `c(idx) = rhs` (text, categorical, or `[]` to delete).
pub fn cat_assign(cur: Value, idx: &[IndexArg], rhs: Value) -> R<Value> {
    let mut c = Cat::from_value(&cur).unwrap();
    let new = if matches!(&rhs, Value::Num(m) if m.dims == [0, 0] && m.kind == Kind::Double) { rhs } else { Value::Num(c.codes_for(&rhs)?) };
    let codes = indexing::assign(Some(Value::Num(c.codes.clone())), idx, new)?.to_matrix()?;
    c.codes = codes;
    Ok(c.value())
}

fn cat_from(v: &Value) -> R<Cat> {
    Cat::from_value(v).ok_or_else(|| LangError::new("Expected a categorical array.", 0))
}

fn cats_col(names: &[String]) -> Value {
    Value::Cell(CellArray::from_dims(vec![names.len(), 1], names.iter().map(|n| Value::chars(n)).collect()))
}

/// Re-maps codes through `map` (old code - 1 -> new code, 0 = undefined).
fn remap(c: &Cat, map: &[usize], cats: Vec<String>) -> Cat {
    let data = c.codes.data.iter().map(|&x| if x == 0.0 { 0.0 } else { map[x as usize - 1] as f64 }).collect();
    Cat { codes: Matrix::from_dims(c.dims(), data), cats, ..c.clone() }
}

fn summary_text(c: &Cat) -> String {
    let counts: Vec<usize> = (1..=c.cats.len()).map(|k| c.codes.data.iter().filter(|&&x| x as usize == k).count()).collect();
    let undefined = c.codes.data.iter().filter(|&&x| x == 0.0).count();
    let mut cols: Vec<(String, String)> = c.cats.iter().cloned().zip(counts.iter().map(|n| n.to_string())).collect();
    if undefined > 0 {
        cols.push((UNDEF.into(), undefined.to_string()));
    }
    let row = c.dims().len() == 2 && c.dims()[0] == 1;
    if row {
        let (mut a, mut b) = (String::new(), String::new());
        for (n, k) in &cols {
            let w = n.chars().count().max(k.len());
            a.push_str(&format!("     {n:<w$} "));
            b.push_str(&format!("     {k:<w$} "));
        }
        return format!("{a}\n{b}\n");
    }
    let w = cols.iter().map(|(n, _)| n.chars().count()).max().unwrap_or(0);
    cols.iter().map(|(n, k)| format!("     {n:<w$}      {k} \n")).collect()
}

fn cat_method(it: &mut Interpreter, m: &str, a: &[Value], nargout: usize) -> Ret {
    if matches!(m, "eq" | "ne" | "lt" | "gt" | "le" | "ge") {
        return compare(m, a);
    }
    if matches!(m, "horzcat" | "vertcat") {
        return one(concat(a.to_vec(), if m == "vertcat" { 1 } else { 2 })?);
    }
    if m == "ismember" {
        let (x, dims) = operand_names(&a[0])?;
        let (set, _) = operand_names(crate::builtins::arg(a, 1, "ismember")?)?;
        let loc: Vec<f64> = x.iter().map(|n| n.as_ref().and_then(|n| set.iter().position(|s| s.as_deref() == Some(n.as_str()))).map(|p| p as f64 + 1.0).unwrap_or(0.0)).collect();
        let tf = logical(dims.clone(), loc.iter().map(|&l| l > 0.0).collect());
        return Ok(Some(vec![tf, Value::Num(Matrix::from_dims(dims, loc))]));
    }
    if m == "isequal" {
        let (x, dx) = operand_names(&a[0])?;
        let all = a[1..].iter().all(|v| operand_names(v).map(|(y, dy)| dy == dx && y == x && x.iter().all(|n| n.is_some())).unwrap_or(false));
        return one(Value::bool(all));
    }
    let c = cat_from(&a[0])?;
    let n = c.codes.data.len();
    let row = c.dims().len() == 2 && c.dims()[0] == 1;
    match m {
        "categories" => one(cats_col(&c.cats)),
        "countcats" => {
            let k = c.cats.len();
            let (r, cols) = if c.codes.is_vector() || n <= 1 { (n, 1) } else { (c.codes.rows(), n / c.codes.rows().max(1)) };
            let mut data = vec![0.0; k * cols];
            for j in 0..cols {
                for i in 0..r {
                    let x = c.codes.data[j * r + i] as usize;
                    if x > 0 {
                        data[j * k + x - 1] += 1.0;
                    }
                }
            }
            one(Value::Num(if row { Matrix::new(1, k, data) } else { Matrix::new(k, cols, data) }))
        }
        "addcats" => {
            let mut c = c;
            for nm in texts(crate::builtins::arg(a, 1, "addcats")?)? {
                if c.cats.contains(&nm) {
                    return rt(format!("'{nm}' is already a category."));
                }
                c.cats.push(nm);
            }
            one(c.value())
        }
        "removecats" => {
            let remove: Vec<String> = match a.get(1) {
                Some(v) => texts(v)?,
                None => c.cats.iter().enumerate().filter(|(k, _)| !c.codes.data.iter().any(|&x| x as usize == k + 1)).map(|(_, s)| s.clone()).collect(),
            };
            let mut map = vec![0; c.cats.len()];
            let mut cats = Vec::new();
            for (k, s) in c.cats.iter().enumerate() {
                if !remove.contains(s) {
                    cats.push(s.clone());
                    map[k] = cats.len();
                }
            }
            one(remap(&c, &map, cats).value())
        }
        "renamecats" => {
            let (old, new) = match a.len() {
                2 => (c.cats.clone(), texts(&a[1])?),
                _ => (texts(crate::builtins::arg(a, 1, "renamecats")?)?, texts(crate::builtins::arg(a, 2, "renamecats")?)?),
            };
            if old.len() != new.len() {
                return rt("Number of new category names must match the number of old category names.");
            }
            let mut c = c;
            for (o, nw) in old.iter().zip(new) {
                let Some(p) = c.code_of(o) else { return rt(format!("'{o}' is not a category.")) };
                c.cats[p - 1] = nw;
            }
            one(c.value())
        }
        "mergecats" => {
            let old = texts(crate::builtins::arg(a, 1, "mergecats")?)?;
            let new = match a.get(2) {
                Some(v) => v.text()?,
                None => old.first().cloned().unwrap_or_default(),
            };
            let first = c.cats.iter().position(|s| old.contains(s)).unwrap_or(c.cats.len());
            let mut map = vec![0; c.cats.len()];
            let mut cats = Vec::new();
            let mut merged = 0;
            for (k, s) in c.cats.iter().enumerate() {
                if old.contains(s) {
                    if k == first {
                        cats.push(new.clone());
                        merged = cats.len();
                    }
                    map[k] = usize::MAX;
                } else {
                    cats.push(s.clone());
                    map[k] = cats.len();
                }
            }
            for x in map.iter_mut() {
                if *x == usize::MAX {
                    *x = merged;
                }
            }
            one(remap(&c, &map, cats).value())
        }
        "reordercats" => {
            let order: Vec<usize> = match a.get(1) {
                None => {
                    let mut ix: Vec<usize> = (0..c.cats.len()).collect();
                    ix.sort_by(|&p, &q| c.cats[p].cmp(&c.cats[q]));
                    ix
                }
                Some(Value::Num(x)) if x.kind != Kind::Char => x.data.iter().map(|&v| v as usize - 1).collect(),
                Some(v) => texts(v)?.iter().map(|s| c.code_of(s).map(|p| p - 1).ok_or_else(|| LangError::new(format!("'{s}' is not a category."), 0))).collect::<R<Vec<_>>>()?,
            };
            let mut sorted = order.clone();
            sorted.sort_unstable();
            if sorted != (0..c.cats.len()).collect::<Vec<_>>() {
                return rt("NEWORDER must be a permutation of the existing categories.");
            }
            let mut map = vec![0; c.cats.len()];
            for (newpos, &old) in order.iter().enumerate() {
                map[old] = newpos + 1;
            }
            let cats = order.iter().map(|&k| c.cats[k].clone()).collect();
            one(remap(&c, &map, cats).value())
        }
        "setcats" => {
            let cats = texts(crate::builtins::arg(a, 1, "setcats")?)?;
            let map: Vec<usize> = c.cats.iter().map(|s| cats.iter().position(|t| t == s).map(|p| p + 1).unwrap_or(0)).collect();
            one(remap(&c, &map, cats).value())
        }
        "isordinal" => one(Value::bool(c.ordinal)),
        "isprotected" => one(Value::bool(c.protected || c.ordinal)),
        "isundefined" | "ismissing" => one(logical(c.dims(), c.codes.data.iter().map(|&x| x == 0.0).collect())),
        "iscategory" => {
            let v = crate::builtins::arg(a, 1, "iscategory")?;
            let names = texts(v)?;
            let dims = if matches!(v, Value::Num(_)) { vec![1, 1] } else { v.dims() };
            one(logical(dims, names.iter().map(|s| c.cats.contains(s)).collect()))
        }
        "transpose" | "ctranspose" => {
            let t = c.codes.transpose();
            one(c.with_codes(t).value())
        }
        "double" | "single" => {
            let data = c.codes.data.iter().map(|&x| if x == 0.0 { f64::NAN } else { x }).collect();
            let mut mx = Matrix::from_dims(c.dims(), data);
            if m == "single" {
                mx = mx.with_kind(Kind::Single);
            }
            one(Value::Num(mx))
        }
        "string" => one(Value::Str(StrArray::from_dims(c.dims(), c.names()))),
        "cellstr" => one(Value::Cell(CellArray::from_dims(c.dims(), c.names().into_iter().map(|s| Value::chars(s.as_deref().unwrap_or(UNDEF))).collect()))),
        "char" => {
            let names: Vec<String> = c.names().into_iter().map(|s| s.unwrap_or_else(|| UNDEF.into())).collect();
            let w = names.iter().map(|s| s.chars().count()).max().unwrap_or(0);
            let r = names.len();
            let mut data = vec![32.0; r * w];
            for (i, s) in names.iter().enumerate() {
                for (j, ch) in s.chars().enumerate() {
                    data[j * r + i] = ch as u32 as f64;
                }
            }
            one(Value::Num(Matrix::new(r, w, data).with_kind(Kind::Char)))
        }
        "numel" => one(Value::num(n as f64)),
        "length" => one(Value::num(if n == 0 { 0.0 } else { *c.dims().iter().max().unwrap() as f64 })),
        "isempty" => one(Value::bool(n == 0)),
        "summary" => {
            it.emit(OutputKind::Text(summary_text(&c)));
            Ok(Some(Vec::new()))
        }
        "unique" => {
            let mut used: Vec<f64> = c.codes.data.iter().copied().filter(|&x| x != 0.0).collect();
            used.sort_by(|p, q| p.partial_cmp(q).unwrap());
            used.dedup();
            let k = used.len();
            let ia: Vec<f64> = used.iter().map(|u| c.codes.data.iter().position(|x| x == u).unwrap() as f64 + 1.0).collect();
            let ic: Vec<f64> = c.codes.data.iter().map(|x| used.iter().position(|u| u == x).map(|p| p as f64 + 1.0).unwrap_or(f64::NAN)).collect();
            let u = c.with_codes(if row { Matrix::new(1, k, used) } else { Matrix::new(k, 1, used) });
            Ok(Some(vec![u.value(), Value::Num(Matrix::col(ia)), Value::Num(Matrix::col(ic))]))
        }
        "sort" => {
            let descend = a[1..].iter().any(|v| v.text().map(|t| t.eq_ignore_ascii_case("descend")).unwrap_or(false));
            let key = |x: f64| if x == 0.0 { f64::INFINITY } else { x };
            let mut ix: Vec<usize> = (0..n).collect();
            ix.sort_by(|&p, &q| {
                let (x, y) = (key(c.codes.data[p]), key(c.codes.data[q]));
                if descend && x.is_finite() && y.is_finite() { y.partial_cmp(&x).unwrap() } else { x.partial_cmp(&y).unwrap() }
            });
            let data = ix.iter().map(|&p| c.codes.data[p]).collect();
            let order = ix.iter().map(|&p| p as f64 + 1.0).collect();
            Ok(Some(vec![c.with_codes(Matrix::from_dims(c.dims(), data)).value(), Value::Num(Matrix::from_dims(c.dims(), order))]))
        }
        "findgroups" => {
            let mut used: Vec<f64> = c.codes.data.iter().copied().filter(|&x| x != 0.0).collect();
            used.sort_by(|p, q| p.partial_cmp(q).unwrap());
            used.dedup();
            let g: Vec<f64> = c.codes.data.iter().map(|x| used.iter().position(|u| u == x).map(|p| p as f64 + 1.0).unwrap_or(f64::NAN)).collect();
            let k = used.len();
            let gn = c.with_codes(Matrix::new(k, 1, used));
            let g = if row { Matrix::new(1, n, g) } else { Matrix::from_dims(c.dims(), g) };
            let _ = nargout;
            Ok(Some(vec![Value::Num(g), gn.value()]))
        }
        "max" | "min" => {
            if !c.ordinal {
                return rt(format!("Unable to compute {m} for categorical arrays that are not ordinal."));
            }
            let vals = c.codes.data.iter().enumerate().filter(|(_, &x)| x != 0.0);
            let best = if m == "max" { vals.max_by(|p, q| p.1.partial_cmp(q.1).unwrap()) } else { vals.min_by(|p, q| p.1.partial_cmp(q.1).unwrap()) };
            let (i, x) = best.map(|(i, &x)| (i as f64 + 1.0, x)).unwrap_or((f64::NAN, 0.0));
            Ok(Some(vec![c.with_codes(Matrix::scalar(x)).value(), Value::num(i)]))
        }
        _ => rt(format!("Undefined function '{m}' for input arguments of type 'categorical'.")),
    }
}

fn cat_body(c: &Cat) -> String {
    let d = c.dims();
    let rows = d[0];
    let cols: usize = d[1..].iter().product();
    let names: Vec<String> = c.names().into_iter().map(|s| s.unwrap_or_else(|| UNDEF.into())).collect();
    let w: Vec<usize> = (0..cols).map(|j| (0..rows).map(|i| names[j * rows + i].chars().count()).max().unwrap_or(0)).collect();
    let mut out = Vec::with_capacity(rows);
    for i in 0..rows {
        let mut line = String::new();
        for j in 0..cols {
            // A single column is not padded.
            let wj = if cols == 1 { 0 } else { w[j] };
            line.push_str(&format!("     {:<wj$} ", names[j * rows + i]));
        }
        out.push(line);
    }
    out.join("\n")
}

fn cat_display(name: &str, c: &Cat) -> String {
    let d = c.dims();
    if c.codes.data.is_empty() {
        return format!("{name} = \n  {} empty categorical array", nd::size_text(&d));
    }
    if c.codes.data.len() == 1 {
        return format!("{name} = categorical\n{}", cat_body(c));
    }
    format!("{name} = {} categorical array\n\n{}", nd::size_text(&d), cat_body(c))
}

// ---- display and dims hooks ----------------------------------------------------------------

/// `name = ...` for an unsuppressed value of these classes.
pub fn display(name: &str, v: &Value, f: NumFormat) -> Option<String> {
    let Value::Object(o) = v else { return None };
    Some(match o.class.name.as_str() {
        MAP => map_display(name, o),
        DICT => dict_display(name, o, f),
        CAT => cat_display(name, &Cat::of(o)),
        _ => return None,
    })
}

/// `disp(v)`.
pub fn disp(v: &Value, f: NumFormat) -> Option<String> {
    let Value::Object(o) = v else { return None };
    match o.class.name.as_str() {
        CAT => Some(cat_body(&Cat::of(o))),
        MAP | DICT => display("", v, f).map(|s| s.trim_start_matches(" = \n").to_string()),
        _ => None,
    }
}

/// Short text of a categorical inside a struct field or cell (`red    blue`), if small.
pub fn inline_text(v: &Value) -> Option<String> {
    let c = Cat::from_value(v)?;
    let d = c.dims();
    let n = c.codes.data.len();
    if n == 0 || d.len() > 2 || d[0] != 1 || n > 10 {
        return Some(format!("{} categorical", nd::size_text(&d)));
    }
    Some(c.names().into_iter().map(|s| s.unwrap_or_else(|| UNDEF.into())).collect::<Vec<_>>().join("    "))
}

/// Element names of a categorical (None = undefined).
pub fn cat_strings(v: &Value) -> Option<Vec<Option<String>>> {
    Cat::from_value(v).map(|c| c.names())
}

pub fn cat_categories(v: &Value) -> Option<Vec<String>> {
    Cat::from_value(v).map(|c| c.cats)
}

pub fn dims(o: &Object) -> Dims {
    match o.class.name.as_str() {
        CAT => match o.get("Codes") {
            Some(Value::Num(m)) => m.dims.clone(),
            _ => vec![0, 0],
        },
        MAP => vec![cell_items(prop(o, "Keys_")).len(), 1],
        _ => vec![1, 1],
    }
}

// ---- keyed indexing ------------------------------------------------------------------------

/// `m(key)` / `d(keys)`.
pub fn lookup(v: &Value, args: &[Value]) -> R<Value> {
    let Value::Object(o) = v else { unreachable!() };
    let [key] = args else { return rt("Only one key may be specified.") };
    if o.class.name == MAP {
        return map_get(o, key);
    }
    Dict::of(o).lookup(key)
}

/// `m(key) = v` / `d(keys) = vals`: returns the updated value (the same handle for a map).
pub fn keyed_assign(cur: Value, args: &[Value], rhs: Value) -> R<Value> {
    let Value::Object(o) = &cur else { unreachable!() };
    let [key] = args else { return rt("Only one key may be specified.") };
    if o.class.name == MAP {
        map_set(o, key, rhs)?;
        return Ok(cur);
    }
    let mut d = Dict::of(o);
    d.insert(key, rhs)?;
    Ok(d.value())
}

// ---- dispatch --------------------------------------------------------------------------------

fn method_call(it: &mut Interpreter, a: &[Value]) -> Ret {
    let m = crate::builtins::arg(a, 0, "matlivre_container_method")?.text()?;
    let nargout = crate::builtins::arg(a, 1, "matlivre_container_method")?.scalar()? as usize;
    let rest = &a[2..];
    let out = if let Some(o) = find_obj(rest, MAP).filter(|_| MAP_METHODS.contains(&m.as_str())) {
        let o = o.clone();
        map_method(it, &m, &o, rest)?
    } else if let Some(o) = find_obj(rest, DICT).filter(|_| DICT_METHODS.contains(&m.as_str())) {
        let o = o.clone();
        dict_method(&m, &o, rest)?
    } else {
        cat_method(it, &m, rest, nargout)?
    };
    let mut out = out.unwrap_or_default();
    out.truncate(nargout.max(1).min(out.len().max(1)));
    one(Value::Cell(CellArray::row(out)))
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], _nargout: usize) -> Ret {
    match name {
        "containers.Map" => new_map(a),
        "dictionary" => new_dict(a),
        "configureDictionary" => configure_dict(a),
        "categorical" => new_categorical(a),
        "iscategorical" => one(Value::bool(a.first().map(is_categorical).unwrap_or(false))),
        "matlivre_container_method" => method_call(it, a),
        _ if name.ends_with(".empty") => empty_of(name.trim_end_matches(".empty"), a),
        _ if NAMES.contains(&name) => {
            let cls = a.first().map(|v| v.class_name()).unwrap_or_else(|| "double".into());
            rt(format!("Undefined function '{name}' for input arguments of type '{cls}'."))
        }
        _ => Ok(None),
    }
}
