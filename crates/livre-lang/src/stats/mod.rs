//! Statistics and Machine Learning Toolbox. Each area has its own module with a `NAMES` list and a
//! `call` dispatcher; `call` here routes a name to its module (see docs/stats-coverage-plan.md).

pub mod anova;
pub mod corr;
pub mod descr;
pub mod dist;
pub mod dist_matrix;
pub mod errtab;
pub mod fitters;
pub mod glmodel;
pub mod hier;
pub mod kmeans;
pub mod ks;
pub mod linmodel;
pub mod lifefit;
pub mod generic;
pub mod classif;
pub mod learners;
pub mod cluster2;
pub mod fits2;
pub mod distobj;
pub mod anova2;
pub mod tests3;
pub mod tests4;
pub mod multcmp;
pub mod anovan;
pub mod tests5;
pub mod tests6;
pub mod tests7;
pub mod tests8;
pub mod tests9;
pub mod tests10;
pub mod tests11;
pub mod cvpart;
pub mod crossval;
pub mod misc;
pub mod searchers;
pub mod neighbors;
pub mod noncentral;
pub mod tests2;
pub mod htest;
pub mod nonpar;
pub mod pca;
pub mod regress;
pub mod sampling;
pub mod rnd;
pub mod util;

use crate::builtins::Ret;
use crate::interp::Interpreter;
use crate::value::*;

/// The function in `name`, if one of the stats modules provides it. Results of functions that do not
/// manage their class follow single precision input (the modules cast their own).
pub fn call(it: &mut Interpreter, name: &str, a: &[Value], nargout: usize) -> Ret {
    if descr::NAMES.contains(&name) {
        return descr::call(it, name, a, nargout);
    }
    if dist::NAMES.contains(&name) {
        return dist::call(it, name, a, nargout);
    }
    if htest::NAMES.contains(&name) {
        return htest::call(it, name, a, nargout);
    }
    if dist_matrix::NAMES.contains(&name) {
        return dist_matrix::call(it, name, a, nargout);
    }
    if hier::NAMES.contains(&name) {
        return hier::call(it, name, a, nargout);
    }
    if pca::NAMES.contains(&name) {
        return pca::call(it, name, a, nargout);
    }
    if regress::NAMES.contains(&name) {
        return regress::call(it, name, a, nargout);
    }
    if kmeans::NAMES.contains(&name) {
        return kmeans::call(it, name, a, nargout);
    }
    if sampling::NAMES.contains(&name) {
        return sampling::call(it, name, a, nargout);
    }
    if nonpar::NAMES.contains(&name) {
        return nonpar::call(it, name, a, nargout);
    }
    if anova::NAMES.contains(&name) {
        return anova::call(it, name, a, nargout);
    }
    if fitters::NAMES.contains(&name) {
        return fitters::call(it, name, a, nargout);
    }
    if ks::NAMES.contains(&name) {
        return ks::call(it, name, a, nargout);
    }
    if linmodel::NAMES.contains(&name) {
        return linmodel::call(it, name, a, nargout);
    }
    if glmodel::NAMES.contains(&name) {
        return glmodel::call(it, name, a, nargout);
    }
    if anova2::NAMES.contains(&name) {
        return anova2::call(it, name, a, nargout);
    }
    if distobj::NAMES.contains(&name) {
        return distobj::call(it, name, a, nargout);
    }
    if searchers::NAMES.contains(&name) {
        return searchers::call(name, a);
    }
    if fits2::NAMES.contains(&name) {
        return fits2::call(it, name, a, nargout);
    }
    if cluster2::NAMES.contains(&name) {
        return cluster2::call(it, name, a, nargout);
    }
    if learners::NAMES.contains(&name) {
        return learners::call(it, name, a, nargout);
    }
    if classif::NAMES.contains(&name) {
        return classif::call(it, name, a, nargout);
    }
    if neighbors::NAMES.contains(&name) {
        return neighbors::call(it, name, a, nargout);
    }
    if generic::NAMES.contains(&name) {
        return generic::call(it, name, a, nargout);
    }
    if lifefit::NAMES.contains(&name) {
        return lifefit::call(it, name, a, nargout);
    }
    if tests2::NAMES.contains(&name) {
        return tests2::call(it, name, a, nargout);
    }
    if misc::NAMES.contains(&name) {
        return misc::call(it, name, a, nargout);
    }
    if rnd::NAMES.contains(&name) {
        return rnd::call(it, name, a, nargout);
    }
    if cvpart::NAMES.contains(&name) {
        return cvpart::call(it, name, a, nargout);
    }
    if crossval::NAMES.contains(&name) {
        return crossval::call(it, name, a, nargout);
    }
    if tests11::NAMES.contains(&name) {
        return tests11::call(it, name, a, nargout);
    }
    if tests10::NAMES.contains(&name) {
        return tests10::call(it, name, a, nargout);
    }
    if tests9::NAMES.contains(&name) {
        return tests9::call(it, name, a, nargout);
    }
    if tests8::NAMES.contains(&name) {
        return tests8::call(it, name, a, nargout);
    }
    if tests7::NAMES.contains(&name) {
        return tests7::call(it, name, a, nargout);
    }
    if tests6::NAMES.contains(&name) {
        return tests6::call(it, name, a, nargout);
    }
    if tests5::NAMES.contains(&name) {
        return tests5::call(it, name, a, nargout);
    }
    if anovan::NAMES.contains(&name) {
        return anovan::call(it, name, a, nargout);
    }
    if multcmp::NAMES.contains(&name) {
        return multcmp::call(it, name, a, nargout);
    }
    if tests4::NAMES.contains(&name) {
        return tests4::call(it, name, a, nargout);
    }
    if tests3::NAMES.contains(&name) {
        return tests3::call(it, name, a, nargout);
    }
    if corr::NAMES.contains(&name) {
        return corr::call(it, name, a, nargout);
    }
    Ok(None)
}

/// Every name the stats modules provide.
pub fn names() -> Vec<&'static str> {
    [descr::NAMES, corr::NAMES, dist::NAMES, rnd::NAMES, htest::NAMES, dist_matrix::NAMES, hier::NAMES, pca::NAMES, regress::NAMES, kmeans::NAMES, sampling::NAMES, nonpar::NAMES, anova::NAMES, fitters::NAMES, ks::NAMES, linmodel::NAMES, glmodel::NAMES, misc::NAMES, tests2::NAMES, lifefit::NAMES, generic::NAMES, neighbors::NAMES, classif::NAMES, learners::NAMES, cluster2::NAMES, fits2::NAMES, searchers::NAMES, distobj::NAMES, anova2::NAMES, tests3::NAMES, tests4::NAMES, multcmp::NAMES, anovan::NAMES, tests5::NAMES, tests6::NAMES, tests7::NAMES, tests8::NAMES, tests9::NAMES, tests10::NAMES, tests11::NAMES, cvpart::NAMES, crossval::NAMES].concat()
}
