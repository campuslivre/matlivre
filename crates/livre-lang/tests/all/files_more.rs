//! The search path (path, addpath, rmpath, restoredefaultpath, matlabpath), what, fileattrib,
//! recycle and toolboxdir, against MATLAB R2023b.

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
fn search_path_functions() {
    // A path with no pathsep character in it: ":" splits "C:\\..." on macOS and Linux.
    let missing = if cfg!(windows) { "C:\\no\\such\\dir" } else { "/no/such/dir" };
    assert_eq!(
        shown(&concat!(
            "d1 = tempname; d2 = tempname; mkdir(d1); mkdir(d2);\n",
            "addpath(d1); addpath(d2, '-end'); parts = strsplit(path, pathsep);\n",
            "isequal(parts{1}, d1), isequal(parts{end}, d2)\n",
            "rmpath(d1); any(strcmp(strsplit(path, pathsep), d1))\n",
            "rmpath('MISSING')\n",
            "p = path(d1, path); q = strsplit(path, pathsep); isequal(q{1}, d1), isequal(p, matlabpath)\n",
            "restoredefaultpath; isempty(path)\n",
            "rmdir(d1); rmdir(d2);\n",
        ).replace("MISSING", missing)),
        [
            "ans = logical\n   1",
            "ans = logical\n   1",
            "ans = logical\n   0",
            &format!("Warning: \"{missing}\" not found in path."),
            "ans = logical\n   1",
            "ans = logical\n   1",
            "ans = logical\n   1",
        ]
    );
}

#[test]
fn what_lists_folder_contents() {
    let out = shown(concat!(
        "d = tempname; mkdir(d); fclose(fopen(fullfile(d,'a.m'),'w')); fclose(fopen(fullfile(d,'z.m'),'w'));\n",
        "fclose(fopen(fullfile(d,'b.txt'),'w')); fclose(fopen(fullfile(d,'c.mat'),'w'));\n",
        "mkdir(fullfile(d,'sub')); mkdir(fullfile(d,'+pkg')); mkdir(fullfile(d,'@cls'));\n",
        "disp(d)\nw = what(d); isequal(w.path, d)\nfieldnames(w)'\nw.m\nw.mlx\nw.classes\nw.packages\n",
        "what(d)\nrmdir(d, 's');\n",
    ));
    let d = out[0].trim_end().to_string();
    assert_eq!(
        out[1..],
        [
            "ans = logical\n   1".to_string(),
            "ans = 1×12 cell\n\n    {'path'}    {'m'}    {'mlapp'}    {'mlx'}    {'mat'}    {'mex'}    {'mdl'}    {'slx'}    {'sfx'}    {'p'}    {'classes'}    {'packages'}".into(),
            "ans = 2×1 cell\n\n    {'a.m'}\n    {'z.m'}".into(),
            "ans =\n\n  0×1 empty cell array".into(),
            "ans = 1×1 cell\n\n    {'cls'}".into(),
            "ans = 1×1 cell\n\n    {'pkg'}".into(),
            format!("\nMATLAB Code files in folder {d}\n\na  z  \n\nMAT-files in folder {d}\n\nc  \n\nClasses in folder {d}\n\ncls  \n\nPackages in folder {d}\n\npkg  \n\n"),
        ]
    );
}

#[test]
fn file_attributes_and_toolbox_folders() {
    let mut expected = vec!["st = logical\n   1".to_string()];
    let checks = if cfg!(windows) {
        vec!["ans = logical\n   1", "ans = logical\n   1", "ans = logical\n   0", "ans = logical\n   1", "ans = logical\n   1"]
    } else {
        vec!["ans = logical\n   1", "ans = logical\n   1", "ans = logical\n   0", "ans = NaN", "ans = logical\n   0"]
    };
    expected.extend(checks.into_iter().map(String::from));
    expected.extend(
        [
            "st2 = logical\n   0",
            "msg2 = 'No such file or directory.'",
            "ans = logical\n   1",
            "ans = logical\n   0",
            "ans = logical\n   1",
            "r = 'off'",
            "tn = 'matlab'",
            "Could not locate the base directory for nosuchtoolbox.\n",
        ]
        .map(String::from),
    );
    assert_eq!(
        shown(concat!(
            "d = tempname; mkdir(d); f = fullfile(d,'a.m'); fclose(fopen(f,'w'));\n",
            "[st, msg] = fileattrib(f)\n",
            "msg.UserRead, msg.UserWrite, msg.directory, msg.archive, isnan(msg.GroupRead)\n",
            "[st2, msg2] = fileattrib(fullfile(d,'nonexist.m'))\n",
            "[~, msg3] = fileattrib(d); msg3.directory\n",
            "fileattrib(f, '-w'); [~, m4] = fileattrib(f); m4.UserWrite\n",
            "fileattrib(f, '+w'); [~, m5] = fileattrib(f); m5.UserWrite\n",
            "r = recycle\n",
            "t = toolboxdir('matlab'); [~, tn] = fileparts(t)\n",
            "try, toolboxdir('nosuchtoolbox'), catch e, disp(e.message), end\n",
            "rmdir(d, 's');\n",
        ))
        .into_iter()
        .filter(|s| !s.starts_with("msg = "))
        .collect::<Vec<_>>(),
        expected
    );
}
