//! Random numbers as MATLAB makes them: the Mersenne Twister (`mt19937ar`, MATLAB's
//! `twister`), `rand`, `randn`, `randi`, `randperm`, `rng` and `RandStream` streams.
//!
//! `rand`, `randi` and `randperm` reproduce MATLAB's sequences for a given seed, and so do
//! the `Polar` and `Inversion` normal transforms. MATLAB's default `Ziggurat` transform is
//! not reproduced: it is computed by inversion, which like MATLAB uses one uniform per normal.

use crate::builtins::{arg, class_option, none, one, size_args, Ret};
use crate::class::ClassDef;
use crate::error::{rt, LangError};
use crate::interp::{Interpreter, OutputKind};
use crate::value::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

type R<T> = Result<T, LangError>;

pub const NAMES: &[&str] = &[
    "rand", "randn", "randi", "randperm", "rng", "RandStream", "RandStream.create", "RandStream.getGlobalStream", "RandStream.setGlobalStream",
    "RandStream.list", "matlivre_random_method",
];

/// RandStream methods, as the function reference names them (for the coverage checklist).
#[allow(dead_code)]
const RS_METHODS: &[&str] = &["reset (RandStream)"];

const N: usize = 624;

/// The 32-bit Mersenne Twister MT19937 with the reference `init_genrand` seeding.
#[derive(Clone)]
pub struct Mt {
    mt: [u32; N],
    idx: usize,
}

impl Mt {
    fn new(seed: u32) -> Mt {
        let mut mt = [0u32; N];
        mt[0] = seed;
        for i in 1..N {
            mt[i] = 1_812_433_253u32.wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_add(i as u32);
        }
        Mt { mt, idx: N }
    }

    fn next_u32(&mut self) -> u32 {
        if self.idx >= N {
            for k in 0..N {
                let y = (self.mt[k] & 0x8000_0000) | (self.mt[(k + 1) % N] & 0x7fff_ffff);
                self.mt[k] = self.mt[(k + 397) % N] ^ (y >> 1) ^ if y & 1 == 1 { 0x9908_b0df } else { 0 };
            }
            self.idx = 0;
        }
        let mut y = self.mt[self.idx];
        self.idx += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// A double in [0, 1) with 53 random bits (`genrand_res53`).
    fn next_f64(&mut self) -> f64 {
        let a = (self.next_u32() >> 5) as f64;
        let b = (self.next_u32() >> 6) as f64;
        (a * 67_108_864.0 + b) / 9_007_199_254_740_992.0
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Transform {
    Ziggurat,
    Polar,
    Inversion,
}

impl Transform {
    fn name(self) -> &'static str {
        match self {
            Transform::Ziggurat => "Ziggurat",
            Transform::Polar => "Polar",
            Transform::Inversion => "Inversion",
        }
    }

    fn parse(s: &str) -> R<Transform> {
        match s.to_lowercase().as_str() {
            "ziggurat" => Ok(Transform::Ziggurat),
            "polar" => Ok(Transform::Polar),
            "inversion" => Ok(Transform::Inversion),
            _ => rt(format!("Unknown NormalTransform: {s}.")),
        }
    }
}

#[derive(Clone)]
pub struct Stream {
    mt: Box<Mt>,
    seed: u32,
    transform: Transform,
    /// The second value of the last Polar pair.
    cached: Option<f64>,
}

/// The global stream, for compiled code that draws from it.
pub(crate) fn global_stream(s: &mut Streams) -> *mut Stream {
    let g = s.global;
    s.get(g) as *mut Stream
}

/// A uniform draw from a stream (compiled `rand`).
///
/// # Safety
/// `p` points to a live stream that nothing else uses during the call.
pub(crate) unsafe fn uniform_of(p: *mut Stream) -> f64 {
    (*p).uniform()
}

/// A copy of the global stream's state, and putting it back (a compiled loop that hands
/// back undoes its draws).
pub(crate) fn save_global(s: &mut Streams) -> Stream {
    let g = s.global;
    s.get(g).clone()
}

pub(crate) fn restore_global(s: &mut Streams, st: Stream) {
    let g = s.global;
    s.map.insert(g, st);
}

impl Stream {
    fn new(seed: u32, transform: Transform) -> Stream {
        // Seed 0 is the reference generator's default seed, 5489.
        Stream { mt: Box::new(Mt::new(if seed == 0 { 5489 } else { seed })), seed, transform, cached: None }
    }

    fn uniform(&mut self) -> f64 {
        self.mt.next_f64()
    }

    fn normal(&mut self) -> f64 {
        match self.transform {
            Transform::Polar => {
                if let Some(v) = self.cached.take() {
                    return v;
                }
                loop {
                    let x = 2.0 * self.uniform() - 1.0;
                    let y = 2.0 * self.uniform() - 1.0;
                    let r = x * x + y * y;
                    if r > 0.0 && r < 1.0 {
                        let f = (-2.0 * r.ln() / r).sqrt();
                        self.cached = Some(y * f);
                        return x * f;
                    }
                }
            }
            _ => -std::f64::consts::SQRT_2 * crate::builtins_specfun::erfcinv(2.0 * self.uniform()),
        }
    }

    /// `randperm(n)` sorts n uniforms; `randperm(n, n)` shuffles inside out; other `k` take
    /// swaps from the end.
    fn perm(&mut self, n: usize, k: Option<usize>) -> Vec<f64> {
        match k {
            None => {
                let u: Vec<f64> = (0..n).map(|_| self.uniform()).collect();
                let mut idx: Vec<usize> = (0..n).collect();
                idx.sort_by(|&i, &j| u[i].partial_cmp(&u[j]).unwrap_or(std::cmp::Ordering::Equal));
                idx.into_iter().map(|i| (i + 1) as f64).collect()
            }
            Some(k) if k == n => {
                let mut a: Vec<f64> = Vec::with_capacity(n);
                for i in 0..n {
                    let j = if i == 0 { 0 } else { (self.uniform() * (i + 1) as f64) as usize };
                    if j == i {
                        a.push((i + 1) as f64);
                    } else {
                        a.push(a[j]);
                        a[j] = (i + 1) as f64;
                    }
                }
                a
            }
            Some(k) => {
                let mut p: Vec<f64> = (1..=n).map(|x| x as f64).collect();
                let mut out = Vec::with_capacity(k);
                for i in (n - k + 1..=n).rev() {
                    let j = ((self.uniform() * i as f64).ceil() as usize).clamp(1, i);
                    p.swap(j - 1, i - 1);
                    out.push(p[i - 1]);
                }
                out
            }
        }
    }

    fn state(&self) -> Vec<f64> {
        self.mt.mt.iter().map(|&w| w as f64).chain(std::iter::once(self.mt.idx as f64)).collect()
    }
}

/// The interpreter's streams: stream 0 starts as the global stream.
pub struct Streams {
    map: HashMap<u64, Stream>,
    objects: HashMap<u64, Value>,
    global: u64,
    next: u64,
}

impl Default for Streams {
    fn default() -> Self {
        Self::new()
    }
}

impl Streams {
    pub fn new() -> Streams {
        let mut map = HashMap::new();
        map.insert(0, Stream::new(0, Transform::Ziggurat));
        Streams { map, objects: HashMap::new(), global: 0, next: 1 }
    }

    fn get(&mut self, id: u64) -> &mut Stream {
        self.map.entry(id).or_insert_with(|| Stream::new(0, Transform::Ziggurat))
    }
}

// ---- RandStream objects ----------------------------------------------------------------------

fn class_def() -> Rc<ClassDef> {
    thread_local! {
        static DEF: RefCell<Option<Rc<ClassDef>>> = const { RefCell::new(None) };
    }
    DEF.with(|d| {
        if let Some(def) = d.borrow().as_ref() {
            return def.clone();
        }
        let src = "classdef RandStream < handle\n  properties\n    Type\n    Seed\n    NormalTransform\n  end\n  properties (Hidden)\n    Id\n  end\n  methods\n    function varargout = reset(obj, varargin)\n      varargout = matlivre_random_method('reset', max(nargout, 1), obj, varargin{:});\n    end\n  end\nend\n";
        let ast = crate::class::parse_classdef(src).expect("built-in class source parses");
        let def = Rc::new(ClassDef::build(ast, &[]));
        *d.borrow_mut() = Some(def.clone());
        def
    })
}

/// The object for stream `id`, made once so every handle to a stream is the same object.
fn object(it: &mut Interpreter, id: u64) -> Value {
    if let Some(v) = it.rng.objects.get(&id) {
        return v.clone();
    }
    let s = it.rng.get(id).clone();
    let v = Value::Object(Object {
        class: class_def(),
        props: Rc::new(RefCell::new(vec![Value::chars("mt19937ar"), seed_value(s.seed), Value::chars(s.transform.name()), Value::num(id as f64)])),
    });
    it.rng.objects.insert(id, v.clone());
    v
}

fn seed_value(seed: u32) -> Value {
    Value::Num(Matrix::scalar(seed as f64).with_kind(Kind::Int(IntClass::U32)))
}

/// Keeps a stream object's visible properties in step with its stream.
fn sync(it: &mut Interpreter, id: u64) {
    let s = it.rng.get(id).clone();
    if let Some(Value::Object(o)) = it.rng.objects.get(&id) {
        let mut p = o.props.borrow_mut();
        p[1] = seed_value(s.seed);
        p[2] = Value::chars(s.transform.name());
    }
}

fn stream_id(v: &Value) -> Option<u64> {
    match v {
        Value::Object(o) if o.class.name == "RandStream" => o.get("Id").and_then(|x| x.scalar().ok()).map(|x| x as u64),
        _ => None,
    }
}

/// The display of a RandStream: `mt19937ar random stream` and its seed and transform.
pub fn stream_text(it_global: Option<u64>, o: &Object) -> Option<String> {
    if o.class.name != "RandStream" {
        return None;
    }
    let id = o.get("Id").and_then(|x| x.scalar().ok()).map(|x| x as u64);
    let seed = o.get("Seed").and_then(|x| x.scalar().ok()).unwrap_or(0.0);
    let t = o.get("NormalTransform").and_then(|x| x.text().ok()).unwrap_or_default();
    let global = if id.is_some() && id == it_global { " (current global stream)" } else { "" };
    Some(format!("mt19937ar random stream{global}\n             Seed: {seed}\n  NormalTransform: {t}"))
}

pub fn global_id(it: &Interpreter) -> u64 {
    it.rng.global
}

fn seed_of(v: &Value) -> R<u32> {
    let x = v.scalar()?;
    if !(0.0..4_294_967_296.0).contains(&x) || x.fract() != 0.0 {
        return rt("The seed must be a nonnegative integer less than 2^32.");
    }
    Ok(x as u32)
}

fn shuffle_seed() -> u32 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    (t % 4_294_967_291) as u32
}

const OTHER_TYPES: &[&str] = &["dsfmt19937", "mcg16807", "mlfg6331_64", "mrg32k3a", "philox4x32_10", "shr3cong", "swb2712", "threefry4x64_20"];

fn new_stream(it: &mut Interpreter, a: &[Value]) -> R<Value> {
    let ty = arg(a, 0, "RandStream")?.text()?;
    match ty.to_lowercase().as_str() {
        "mt19937ar" | "twister" => {}
        t if OTHER_TYPES.contains(&t) => return rt(format!("The {ty} generator is not supported in MatLivre; use mt19937ar.")),
        _ => return rt(format!("Unknown RNG type: {ty}.")),
    }
    let (mut seed, mut transform) = (0u32, Transform::Ziggurat);
    for pair in a[1..].chunks(2) {
        let key = pair[0].text()?.to_lowercase();
        let Some(v) = pair.get(1) else { return rt("Name-value arguments must come in pairs.") };
        match key.as_str() {
            "seed" => seed = if v.is_text() && v.text()?.eq_ignore_ascii_case("shuffle") { shuffle_seed() } else { seed_of(v)? },
            "normaltransform" | "randntransform" => transform = Transform::parse(&v.text()?)?,
            "numstreams" | "streamindices" | "celloutput" | "parallelsubstreams" => {}
            _ => return rt(format!("Invalid parameter name: {}.", pair[0].text()?)),
        }
    }
    let id = it.rng.next;
    it.rng.next += 1;
    it.rng.map.insert(id, Stream::new(seed, transform));
    Ok(object(it, id))
}

// ---- rng -------------------------------------------------------------------------------------

fn settings(s: &Stream) -> Value {
    let mut st = StructArray::scalar();
    st.set("Type", Value::chars("twister"));
    st.set("Seed", seed_value(s.seed));
    st.set("State", Value::Num(Matrix::from_dims(vec![N + 1, 1], s.state()).with_kind(Kind::Int(IntClass::U32))));
    Value::Struct(st)
}

fn restore(s: &StructArray) -> R<Stream> {
    let seed = s.get("Seed").map(seed_of).transpose()?.unwrap_or(0);
    let mut st = Stream::new(seed, Transform::Ziggurat);
    if let Some(Value::Num(m)) = s.get("State") {
        if m.numel() == N + 1 {
            for i in 0..N {
                st.mt.mt[i] = m.data[i] as u32;
            }
            st.mt.idx = (m.data[N] as usize).min(N);
        }
    }
    Ok(st)
}

fn rng(it: &mut Interpreter, a: &[Value], nargout: usize) -> Ret {
    let g = it.rng.global;
    let previous = settings(it.rng.get(g));
    let generator_ok = |t: &str| -> R<()> {
        match t.to_lowercase().as_str() {
            "twister" | "mt19937ar" => Ok(()),
            "simdtwister" | "combrecursive" | "multfibonacci" | "philox" | "threefry" | "v4" | "v5uniform" | "v5normal" => {
                rt(format!("The {t} generator is not supported in MatLivre; use twister."))
            }
            _ => rt(format!("Unknown generator: {t}.")),
        }
    };
    let fresh = match a.first() {
        None => None,
        Some(Value::Struct(s)) => Some(restore(s)?),
        Some(v) if v.is_text() => match v.text()?.to_lowercase().as_str() {
            "default" => Some(Stream::new(0, Transform::Ziggurat)),
            "shuffle" => {
                if let Some(t) = a.get(1) {
                    generator_ok(&t.text()?)?;
                }
                Some(Stream::new(shuffle_seed(), Transform::Ziggurat))
            }
            t => {
                generator_ok(t)?;
                Some(Stream::new(0, Transform::Ziggurat))
            }
        },
        Some(v) => {
            if let Some(t) = a.get(1) {
                generator_ok(&t.text()?)?;
            }
            Some(Stream::new(seed_of(v)?, Transform::Ziggurat))
        }
    };
    if let Some(s) = fresh {
        it.rng.map.insert(g, s);
        sync(it, g);
        if nargout == 0 {
            return none();
        }
    }
    one(previous)
}

// ---- rand, randn, randi, randperm -----------------------------------------------------------

fn generate(it: &mut Interpreter, name: &str, a: &[Value]) -> Ret {
    let (id, a) = match a.first().and_then(stream_id) {
        Some(id) => (id, &a[1..]),
        None => (it.rng.global, a),
    };
    match name {
        "rand" | "randn" => {
            let (rest, kind) = class_option(a)?;
            let d = size_args(&rest)?;
            let n = crate::nd::numel(&d);
            let s = it.rng.get(id);
            let data = (0..n).map(|_| if name == "rand" { s.uniform() } else { s.normal() }).collect();
            one(Value::Num(Matrix::from_dims(d, data).cast_to(kind)))
        }
        "randi" => {
            let lim = arg(a, 0, name)?.to_matrix()?;
            let (lo, hi) = if lim.numel() >= 2 { (lim.data[0], lim.data[1]) } else { (1.0, lim.data.first().copied().unwrap_or(0.0)) };
            if lo.fract() != 0.0 || hi.fract() != 0.0 || hi < lo {
                return rt("The first input must be a positive integer scalar or a 2-element vector of integers with the first no greater than the second.");
            }
            let (rest, kind) = class_option(&a[1..])?;
            let d = size_args(&rest)?;
            let n = crate::nd::numel(&d);
            let s = it.rng.get(id);
            let range = hi - lo + 1.0;
            let data = (0..n).map(|_| (lo + (s.uniform() * range).floor()).min(hi)).collect();
            one(Value::Num(Matrix::from_dims(d, data).cast_to(kind)))
        }
        _ => {
            let n = arg(a, 0, name)?.as_usize()?;
            let k = a.get(1).map(|v| v.as_usize()).transpose()?;
            if k.is_some_and(|k| k > n) {
                return rt("K must be less than or equal to N.");
            }
            let v = it.rng.get(id).perm(n, k);
            one(Value::Num(Matrix::row(v)))
        }
    }
}

pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    match name {
        "rand" | "randn" | "randi" | "randperm" => generate(it, name, a),
        "rng" => rng(it, a, nargout),
        "RandStream" | "RandStream.create" => one(new_stream(it, a)?),
        "RandStream.getGlobalStream" => {
            let g = it.rng.global;
            one(object(it, g))
        }
        "RandStream.setGlobalStream" => {
            let Some(id) = a.first().and_then(stream_id) else { return rt("The input must be a RandStream.") };
            let old = it.rng.global;
            let prev = object(it, old);
            it.rng.global = id;
            one(prev)
        }
        "RandStream.list" => {
            it.emit(OutputKind::Text(
                "The following random number generator algorithms are available:\nmt19937ar:        Mersenne Twister with Mersenne prime 2^19937-1\n".into(),
            ));
            none()
        }
        "matlivre_random_method" => {
            // reset(s) or reset(s, seed).
            let Some(id) = a.get(2).and_then(stream_id) else { return rt("Invalid RandStream.") };
            let s = it.rng.get(id);
            let seed = match a.get(3) {
                Some(v) => seed_of(v)?,
                None => s.seed,
            };
            let t = s.transform;
            it.rng.map.insert(id, Stream::new(seed, t));
            sync(it, id);
            one(Value::Cell(CellArray::row(Vec::new())))
        }
        _ => Ok(None),
    }
}
