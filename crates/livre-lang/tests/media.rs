//! Images and WAV audio against MATLAB R2023b.

use livre_lang::{Interpreter, OutputKind};

fn run(src: &str) -> (Interpreter, Vec<String>) {
    let mut it = Interpreter::new();
    let _ = it.run_source(src);
    let out = it
        .take_outputs()
        .into_iter()
        .filter_map(|o| match o.kind {
            OutputKind::Error(e) => Some(format!("Error: {e}")),
            _ => None,
        })
        .collect();
    (it, out)
}

fn data(it: &Interpreter, name: &str) -> Vec<f64> {
    it.get_var(name).unwrap().to_matrix().unwrap().data
}

#[test]
fn images_round_trip_like_matlab() {
    let d = std::env::temp_dir().join(format!("matlivre-img-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let p = |n: &str| d.join(n).display().to_string().replace('\'', "''");
    let src = format!(
        concat!(
            "A = uint8(reshape(0:23, 2, 4, 3) * 10); imwrite(A, '{rgb}'); same_png = isequal(A, imread('{rgb}'));\n",
            "imwrite(A, '{bmp}'); same_bmp = isequal(A, imread('{bmp}'));\n",
            "imwrite([0 0.5; 1 0.25], '{dbl}'); dd = imread('{dbl}');\n",
            "imwrite(logical([1 0; 0 1]), '{lg}'); l = imread('{lg}'); lc = class(l);\n",
            "info = imfinfo('{rgb}'); wh = [info.Width info.Height info.BitDepth]; ct = info.ColorType;\n",
            "[X, map] = imread('{dbl}'); ms = size(map);\n",
        ),
        rgb = p("rgb.png"),
        bmp = p("rgb.bmp"),
        dbl = p("d.png"),
        lg = p("l.png"),
    );
    let (it, errs) = run(&src);
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(data(&it, "same_png"), [1.0]);
    assert_eq!(data(&it, "same_bmp"), [1.0]);
    assert_eq!(data(&it, "dd"), [0.0, 255.0, 128.0, 64.0]);
    assert_eq!(it.get_var("lc").unwrap().text().unwrap(), "logical");
    assert_eq!(data(&it, "wh"), [4.0, 2.0, 24.0]);
    assert_eq!(it.get_var("ct").unwrap().text().unwrap(), "truecolor");
    assert_eq!(data(&it, "ms"), [0.0, 0.0]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn wav_audio_and_mu_law_match_matlab() {
    let d = std::env::temp_dir().join(format!("matlivre-wav-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let w = d.join("t.wav").display().to_string().replace('\'', "''");
    let src = format!(
        concat!(
            "y = sin(2*pi*440*(0:99)'/8000); audiowrite('{w}', y, 8000);\n",
            "[z, fs] = audioread('{w}'); err = max(abs(z - y)); zi = audioread('{w}', 'native'); zi3 = double(zi(1:3))';\n",
            "ai = audioinfo('{w}'); a = [ai.SampleRate ai.NumChannels ai.TotalSamples ai.BitsPerSample ai.Duration];\n",
            "mu = lin2mu([-1 0 0.5 1]); ln = mu2lin(mu);\n",
        ),
        w = w
    );
    let (it, errs) = run(&src);
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(data(&it, "fs"), [8000.0]);
    assert!((data(&it, "err")[0] - 2.9924e-05).abs() < 1e-8);
    assert_eq!(data(&it, "zi3"), [0.0, 11099.0, 20887.0]);
    assert_eq!(data(&it, "a"), [8000.0, 1.0, 100.0, 16.0, 0.0125]);
    assert_eq!(data(&it, "mu"), [0.0, 255.0, 143.0, 128.0]);
    let ln = data(&it, "ln");
    for (a, b) in ln.iter().zip([-0.9803, 0.0, 0.5116, 0.9803]) {
        assert!((a - b).abs() < 1e-4, "{ln:?}");
    }
    let _ = std::fs::remove_dir_all(&d);
}
