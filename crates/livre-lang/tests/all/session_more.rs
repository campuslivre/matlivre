//! diary, more, inmem, memory, localfunctions, canUseGPU, canUseParallelPool, superiorto and
//! T.empty, against MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind};

fn shown(src: &str) -> Vec<String> {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    it.take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Value { text, .. } => Some(text),
            OutputKind::Text(t) => Some(t),
            OutputKind::Warning(w) => Some(format!("Warning: {w}")),
            OutputKind::Error(e) => Some(format!("Error: {e}")),
            _ => None,
        })
        .collect()
}

#[test]
fn environment_queries() {
    // `memory` exists on Windows only, as in MATLAB.
    let mut want: Vec<&str> = vec![
            "g = logical\n   0",
            "p = logical\n   0",
            "ans = 'cell'",
            "ans = 'cell'",
            "ans = 'cell'",
    ];
    if cfg!(windows) {
        want.extend([
            "ans = 3×1 cell\n\n    {'MaxPossibleArrayBytes'}\n    {'MemAvailableAllArrays'}\n    {'MemUsedMATLAB'        }",
            "ans = 3×1 cell\n\n    {'VirtualAddressSpace'}\n    {'SystemMemory'       }\n    {'PhysicalMemory'     }",
            "ans = 2×1 cell\n\n    {'Available'}\n    {'Total'    }",
        ]);
    }
    want.push("Function 'SUPERIORTO' can only be called from within a method.\n");
    assert_eq!(
        shown(concat!(
            "g = canUseGPU\np = canUseParallelPool\nmore on\nmore off\nmore(20)\n",
            "m = inmem; class(m)\n[m, mx, c] = inmem; class(mx), class(c)\n",
            "if ispc, [uv, sv] = memory; fieldnames(uv), fieldnames(sv), fieldnames(sv.PhysicalMemory), end\n",
            "try, superiorto('double'), catch e, disp(e.message), end\n",
        )),
        want
    );
}

#[test]
fn diary_copies_output() {
    assert_eq!(
        shown(concat!(
            "f = tempname;\ndiary(f)\ndisp('one')\nx = 5\ndiary off\ndisp('two')\n",
            "diary(f)\ndisp('three')\ndiary off\n",
            "fid = fopen(f); t = fread(fid, '*char')'; fclose(fid);\nstrcmp(t, sprintf('one\\nx = 5\\nthree\\n'))\n",
            "get(0, 'Diary')\nstrcmp(get(0, 'DiaryFile'), f)\ndiary on\nget(0, 'Diary')\ndiary off\ndelete(f)\n",
        )),
        [
            "one\n",
            "x = 5",
            "two\n",
            "three\n",
            "ans = logical\n   1",
            "ans = 'off'",
            "ans = logical\n   1",
            "ans = 'on'",
        ]
    );
}

#[test]
fn local_functions() {
    let dir = std::env::temp_dir().join(format!("matlivre_lf_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("lf_demo.m"),
        "function fns = lf_demo()\nfns = localfunctions;\nend\nfunction y = twice(x)\ny = 2*x;\nend\nfunction y = thrice(x)\ny = 3*x;\nend\n",
    )
    .unwrap();
    let mut it = Interpreter::new();
    it.set_search_path(vec![dir.clone()]);
    let _ = it.run_source("fns = lf_demo()\nfns{2}(3)\nsize(fns)\nlf = localfunctions\nfunction z = helper1(x)\nz = x;\nend\n");
    let out: Vec<String> = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Value { text, .. } => Some(text),
            OutputKind::Error(e) => Some(format!("Error: {e}")),
            _ => None,
        })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        out,
        [
            "fns = 2×1 cell\n\n    { @twice}\n    {@thrice}",
            "ans = 9",
            "ans = 1×2\n\n     2     1",
            "lf = 1×1 cell\n\n    {@helper1}",
        ]
    );
}

#[test]
fn empty_arrays_of_a_class() {
    assert_eq!(
        shown(concat!(
            "x = double.empty\ny = double.empty(0,3)\nz = int8.empty(2,0,3)\nl = logical.empty(1,0)\n",
            "try, double.empty(1,2), catch e, disp(e.message), end\n",
            "d = datetime.empty\nclass(d)\nu = duration.empty(0,2); size(u), class(u)\n",
        )),
        [
            "x = []",
            "y = 0×3 empty double matrix",
            "z = 2×0×3 empty int8 array",
            "l = 1×0 empty logical array",
            "At least one dimension must be zero.\n",
            "d = \n  0×0 empty datetime array",
            "ans = 'datetime'",
            "ans = 1×2\n\n     0     2",
            "ans = 'duration'",
        ]
    );
}
