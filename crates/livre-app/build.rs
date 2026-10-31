//! Embeds the app icon and the version information (the "MatLivre" name Windows shows in the
//! taskbar, Task Manager and the file's properties) in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/matlivre.rc");
    println!("cargo:rerun-if-changed=assets/matlivre.ico");
    // Until the .ico has been generated (see assets/matlivre.rc) the exe has no icon.
    let ico = std::path::Path::new("assets/matlivre.ico").exists();
    if !ico || std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // assets/matlivre.rc holds the icon; the version block is generated from the package version.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let icon_rc = std::fs::read_to_string("assets/matlivre.rc").unwrap();
    let icon_rc = icon_rc.replace("\"matlivre.ico\"", &format!("\"{}\"", format!("{manifest}/assets/matlivre.ico").replace('\\', "/")));
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let mut parts: Vec<u32> = version.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).take(3).collect();
    parts.resize(4, 0);
    let numeric = parts.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",");
    let rc = format!(
        r#"{icon_rc}
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "MatLivre"
      VALUE "FileDescription", "MatLivre"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "matlivre"
      VALUE "OriginalFilename", "matlivre.exe"
      VALUE "ProductName", "MatLivre"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("matlivre.rc");
    std::fs::write(&out, rc).unwrap();
    embed_resource::compile(&out, embed_resource::NONE).manifest_optional().unwrap();
}
