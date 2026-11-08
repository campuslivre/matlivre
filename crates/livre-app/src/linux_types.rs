//! Linux file types. The AppImage that Velopack makes has no way to declare the files it opens,
//! so the running AppImage registers them for the current user: a MIME definition, a desktop
//! entry (so the file manager offers MatLivre under Open With) and icons. Nothing is changed
//! outside an AppImage (a cargo run or a tarball install leaves the system alone), and the
//! default application for a type is never changed: the user picks it.
//!
//! Keep FILE_TYPES in step with file-types under [package.metadata.release-manager] in
//! crates/livre-app/Cargo.toml (the list Windows and macOS use).

/// Each type: the extension, its description, its MIME type, the icon's name in the icon theme and
/// the icon (SVG).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const FILE_TYPES: &[(&str, &str, &str, &str, &str)] = &[
    ("mlx", "MatLivre Live Script", "application/vnd.mathworks.matlab.mlx", "application-vnd.mathworks.matlab.mlx", include_str!("../assets/mlx.svg")),
    // text/x-matlab is the type shared-mime-info gives MATLAB code; its *.m glob is also claimed by
    // Objective-C, so a higher weight makes .m files MATLAB scripts for this user.
    ("m", "MatLivre Script", "text/x-matlab", "text-x-matlab", include_str!("../assets/m.svg")),
];

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const APP_ICON: &str = include_str!("../assets/matlivre.svg");

/// The shared-mime-info definition of the file types (~/.local/share/mime/packages/matlivre.xml).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn mime_xml() -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n");
    for (ext, description, mime, icon, _) in FILE_TYPES {
        let parent = if *mime == "text/x-matlab" { "text/plain" } else { "application/zip" };
        xml.push_str(&format!(
            "  <mime-type type=\"{mime}\">\n    <comment>{description}</comment>\n    <sub-class-of type=\"{parent}\"/>\n    <icon name=\"{icon}\"/>\n    <glob pattern=\"*.{ext}\" weight=\"60\"/>\n  </mime-type>\n"
        ));
    }
    xml.push_str("</mime-info>\n");
    xml
}

/// A desktop entry that starts the AppImage at `appimage` with the file to open.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn desktop_entry(appimage: &str) -> String {
    // Exec quoting: inside double quotes, " ` $ and \ are escaped with a backslash (and the
    // backslash itself is doubled again by the desktop entry's own string escaping).
    let mut exec = String::new();
    for c in appimage.chars() {
        match c {
            '\\' => exec.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                exec.push_str("\\\\");
                exec.push(c);
            }
            '%' => exec.push_str("%%"),
            _ => exec.push(c),
        }
    }
    let mimes: String = FILE_TYPES.iter().map(|t| format!("{};", t.2)).collect();
    format!(
        "[Desktop Entry]\nType=Application\nName=MatLivre\nComment=Live script editor and runtime for MATLAB code\nExec=\"{exec}\" %f\nIcon=matlivre\nTerminal=false\nCategories=Education;Science;Math;Development;\nMimeType={mimes}\nStartupWMClass=matlivre\n"
    )
}

/// Registers the file types for the current user when running as an AppImage. Cheap to call at
/// every start: files that are already right are not rewritten.
#[cfg(target_os = "linux")]
pub fn register() {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    fn write_if_changed(path: &Path, text: &str) -> bool {
        if std::fs::read_to_string(path).map(|old| old == text).unwrap_or(false) {
            return false;
        }
        if let Some(dir) = path.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                return false;
            }
        }
        std::fs::write(path, text).is_ok()
    }

    let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) else { return };
    let share = match std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()) {
        Some(p) => p,
        None => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(".local/share"),
            None => return,
        },
    };

    let mime_changed = write_if_changed(&share.join("mime/packages/matlivre.xml"), &mime_xml());
    let desktop_changed = write_if_changed(&share.join("applications/matlivre.desktop"), &desktop_entry(&appimage.to_string_lossy()));
    let mut icons_changed = write_if_changed(&share.join("icons/hicolor/scalable/apps/matlivre.svg"), APP_ICON);
    for (_, _, _, icon, svg) in FILE_TYPES {
        icons_changed |= write_if_changed(&share.join(format!("icons/hicolor/scalable/mimetypes/{icon}.svg")), svg);
    }

    let run = |program: &str, dir: &Path| {
        let _ = Command::new(program).arg(dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    };
    if mime_changed {
        run("update-mime-database", &share.join("mime"));
    }
    if desktop_changed {
        run("update-desktop-database", &share.join("applications"));
    }
    if icons_changed {
        let _ = Command::new("gtk-update-icon-cache")
            .args(["-q", "-t", "-f"])
            .arg(share.join("icons/hicolor"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(not(target_os = "linux"))]
pub fn register() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_definition_has_a_glob_for_every_type() {
        let xml = mime_xml();
        assert!(xml.contains("<glob pattern=\"*.mlx\" weight=\"60\"/>"));
        assert!(xml.contains("<glob pattern=\"*.m\" weight=\"60\"/>"));
        assert!(xml.contains("application/vnd.mathworks.matlab.mlx"));
    }

    #[test]
    fn desktop_entry_lists_the_types_and_quotes_the_path() {
        let entry = desktop_entry("/home/a b/MatLivre \"x\".AppImage");
        assert!(entry.contains("MimeType=application/vnd.mathworks.matlab.mlx;text/x-matlab;"));
        assert!(entry.contains("Exec=\"/home/a b/MatLivre \\\\\"x\\\\\".AppImage\" %f"));
    }
}
