//! Loads the live scripts shipped with a local MATLAB install, when one is present.

use std::path::Path;

use livre_doc::{read_mlx, read_mlx_bytes, write_mlx_bytes, BlockKind};

const SAMPLES: &[&str] = &[
    r"C:\Program Files\MATLAB\R2023b\toolbox\matlab\demos\SolarPanelEstimator.mlx",
    r"C:\Program Files\MATLAB\R2023b\toolbox\matlab\demos\RealTimeDashboard.mlx",
    r"C:\Program Files\MATLAB\R2023b\toolbox\matlab\external\interfaces\cpp\internal\publishInterfaceWorkflowTemplate.mlx",
];

#[test]
fn reads_shipped_live_scripts() {
    for s in SAMPLES {
        let p = Path::new(s);
        if !p.exists() {
            eprintln!("skipping {s} (not installed)");
            continue;
        }
        let doc = read_mlx(p).unwrap_or_else(|e| panic!("{s}: {e}"));
        assert!(doc.blocks.iter().any(|b| b.kind == BlockKind::Code), "{s} has code");
        // Our writer must round-trip what we read.
        let back = read_mlx_bytes(&write_mlx_bytes(&doc).unwrap()).unwrap();
        assert_eq!(doc.blocks.len(), back.blocks.len());
        for (a, b) in doc.blocks.iter().zip(&back.blocks) {
            assert_eq!((a.kind, &a.text), (b.kind, &b.text));
        }
        eprintln!("{s}: {} blocks, {} sections", doc.blocks.len(), doc.sections().len());
    }
}
