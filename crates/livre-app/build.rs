//! Embeds the app icon in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/matlivre.rc");
    println!("cargo:rerun-if-changed=assets/matlivre.ico");
    // Until the .ico has been generated (see assets/matlivre.rc) the exe has no icon.
    let ico = std::path::Path::new("assets/matlivre.ico").exists();
    if ico && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/matlivre.rc", embed_resource::NONE).manifest_optional().unwrap();
    }
}
