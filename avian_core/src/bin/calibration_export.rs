// 6.3: regenerate `analysis/calibration_export.json` from the compiled
// constants so the Python analysis scripts read the SAME source of truth.
//
// Usage:
//   cargo run -p avian_core --bin calibration_export
//
// Writes `<workspace>/analysis/calibration_export.json` by default, or to a
// path passed as the first CLI argument.
use std::path::PathBuf;

fn main() {
    let json = avian_core::calibration::calibration_export_json();
    let text = serde_json::to_string_pretty(&json).expect("serialize calibration export");

    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            manifest.join("..").join("analysis").join("calibration_export.json")
        });

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create export directory");
    }
    std::fs::write(&path, text).expect("write calibration export");
    println!("wrote {}", path.display());
}
