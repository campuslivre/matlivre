//! Signal Processing Toolbox. Each area has its own module with a `NAMES` list and a `call`
//! dispatcher, like the other builtin modules; `call` here routes a name to its module.
//!
//! The toolbox is not part of the MATLAB installation the project records outputs from, so the
//! definitions follow the function reference at mathworks.com/help/signal and the tests check
//! those documented formulas (see docs/signal-coverage-plan.md).

pub mod analog;
pub mod ar;
pub mod cepstra;
pub mod change;
pub mod bandwidth;
pub mod modeling;
pub mod pulses;
pub mod zcrate;
pub mod chgpts;
pub mod envelope;
pub mod invfreq;
pub mod stmcb;
pub mod framing;
pub mod stfts;
pub mod waveforms2;
pub mod conversions;
pub mod db;
pub mod filtering;
pub mod elliptic;
pub mod fanalysis;
pub mod fir;
pub mod firdes;
pub mod iir;
pub mod lapack_eig;
pub mod lpc;
pub mod lpc2;
pub mod measure2;
pub mod measures;
pub mod order;
pub mod peaks;
pub mod poly;
pub mod pspec;
pub mod rational;
pub mod remez;
pub mod resamp;
pub mod response;
pub mod smoothing;
pub mod sos;
pub mod ssconv;
pub mod spectral;
pub mod statespace;
pub mod transforms;
pub mod transforms2;
pub mod waveforms;
pub mod windows;

use crate::builtins::Ret;
use crate::interp::Interpreter;
use crate::value::*;

/// The function in `name`, if one of the signal modules provides it. Results are single precision
/// when any argument is.
pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    let result = dispatch(it, name, a, nargout)?;
    // These functions decide the class of each output themselves.
    let single = !OWN_PRECISION.contains(&name) && a.iter().any(|v| matches!(v, Value::Num(m) if m.kind == Kind::Single));
    Ok(result.map(|vals| if single { vals.into_iter().map(to_single).collect() } else { vals }))
}

/// Functions whose outputs do not all follow a single-precision input.
const OWN_PRECISION: &[&str] = &["tripuls", "gauspuls", "zp2tf", "impinvar", "buttord", "cheb1ord", "cheb2ord", "ellipord", "kaiserord", "fir2", "firls", "firpm", "firpmord", "gaussdesign", "rcosdesign", "firtype", "filtord", "filternorm", "impzlength", "enbw", "finddelay", "cusum", "dtw", "edr", "alignsignals", "decimate", "interp", "zerocrossrate", "findchangepts", "envelope", "invfreqz", "invfreqs", "stmcb", "buffer", "shiftdata", "unshiftdata", "uencode", "udecode", "taylorwin", "pulstran", "vco", "modulate", "dpss", "marcumq", "rainflow", "demod", "iscola", "stft", "istft"];

/// A matrix with its values rounded to single precision (the class is left to the caller).
pub(crate) fn cast_single_values(mut m: Matrix) -> Matrix {
    for x in m.data.iter_mut() {
        *x = *x as f32 as f64;
    }
    m
}

pub(crate) fn to_single(v: Value) -> Value {
    match v {
        Value::Num(mut m) if m.kind == Kind::Double => {
            m.kind = Kind::Single;
            for x in m.data.iter_mut() {
                *x = *x as f32 as f64;
            }
            if let Some(im) = m.im.as_mut() {
                for x in im.iter_mut() {
                    *x = *x as f32 as f64;
                }
            }
            Value::Num(m)
        }
        other => other,
    }
}

fn dispatch(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if windows::NAMES.contains(&name) {
        return windows::call(it, name, a, nargout);
    }
    if waveforms::NAMES.contains(&name) {
        return waveforms::call(it, name, a, nargout);
    }
    if db::NAMES.contains(&name) {
        return db::call(it, name, a, nargout);
    }
    if analog::NAMES.contains(&name) {
        return analog::call(it, name, a, nargout);
    }
    if iir::NAMES.contains(&name) {
        return iir::call(it, name, a, nargout);
    }
    if fir::NAMES.contains(&name) {
        return fir::call(it, name, a, nargout);
    }
    if response::NAMES.contains(&name) {
        return response::call(it, name, a, nargout);
    }
    if conversions::NAMES.contains(&name) {
        return conversions::call(it, name, a, nargout);
    }
    if measures::NAMES.contains(&name) {
        return measures::call(it, name, a, nargout);
    }
    if spectral::NAMES.contains(&name) {
        return spectral::call(it, name, a, nargout);
    }
    if transforms::NAMES.contains(&name) {
        return transforms::call(it, name, a, nargout);
    }
    if peaks::NAMES.contains(&name) {
        return peaks::call(it, name, a, nargout);
    }
    if smoothing::NAMES.contains(&name) {
        return smoothing::call(it, name, a, nargout);
    }
    if filtering::NAMES.contains(&name) {
        return filtering::call(it, name, a, nargout);
    }
    if ssconv::NAMES.contains(&name) {
        return ssconv::call(it, name, a, nargout);
    }
    if stfts::NAMES.contains(&name) {
        return stfts::call(it, name, a, nargout);
    }
    if waveforms2::NAMES.contains(&name) {
        return waveforms2::call(it, name, a, nargout);
    }
    if framing::NAMES.contains(&name) {
        return framing::call(it, name, a, nargout);
    }
    if stmcb::NAMES.contains(&name) {
        return stmcb::call(it, a, nargout);
    }
    if invfreq::NAMES.contains(&name) {
        return invfreq::call(it, name, a, nargout);
    }
    if envelope::NAMES.contains(&name) {
        return envelope::call(it, a, nargout);
    }
    if chgpts::NAMES.contains(&name) {
        return chgpts::call(a, nargout);
    }
    if zcrate::NAMES.contains(&name) {
        return zcrate::call(a, nargout);
    }
    if pulses::NAMES.contains(&name) {
        return pulses::call(name, a, nargout);
    }
    if modeling::NAMES.contains(&name) {
        return modeling::call(it, name, a, nargout);
    }
    if bandwidth::NAMES.contains(&name) {
        return bandwidth::call(it, name, a, nargout);
    }
    if resamp::NAMES.contains(&name) {
        return resamp::call(it, name, a, nargout);
    }
    if change::NAMES.contains(&name) {
        return change::call(it, name, a, nargout);
    }
    if pspec::NAMES.contains(&name) {
        return pspec::call(it, name, a, nargout);
    }
    if cepstra::NAMES.contains(&name) {
        return cepstra::call(it, name, a, nargout);
    }
    if transforms2::NAMES.contains(&name) {
        return transforms2::call(it, name, a, nargout);
    }
    if ar::NAMES.contains(&name) {
        return ar::call(it, name, a, nargout);
    }
    if lpc2::NAMES.contains(&name) {
        return lpc2::call(it, name, a, nargout);
    }
    if measure2::NAMES.contains(&name) {
        return measure2::call(it, name, a, nargout);
    }
    if fanalysis::NAMES.contains(&name) {
        return fanalysis::call(it, name, a, nargout);
    }
    if remez::NAMES.contains(&name) {
        return remez::call(it, name, a, nargout);
    }
    if firdes::NAMES.contains(&name) {
        return firdes::call(it, name, a, nargout);
    }
    if order::NAMES.contains(&name) {
        return order::call(it, name, a, nargout);
    }
    if elliptic::NAMES.contains(&name) {
        return elliptic::call(it, name, a, nargout);
    }
    if rational::NAMES.contains(&name) {
        return rational::call(it, name, a, nargout);
    }
    if sos::NAMES.contains(&name) {
        return sos::call(it, name, a, nargout);
    }
    if lpc::NAMES.contains(&name) {
        return lpc::call(it, name, a, nargout);
    }
    Ok(None)
}
