//! The Claude Code CLI installed on this computer, which the chat panel and code completion
//! drive in its headless (`-p`) mode. It runs on the user's own Claude Code sign-in.

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The `claude` executable: on the PATH, or where the native installer and npm put it.
pub fn find() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        dirs.push(PathBuf::from(&home).join(".local").join("bin"));
        dirs.push(PathBuf::from(&home).join(".claude").join("local"));
    }
    #[cfg(target_os = "macos")]
    for d in ["/opt/homebrew/bin", "/usr/local/bin"] {
        dirs.push(PathBuf::from(d));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    dirs.iter().flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file())
}

/// A `claude` command with piped standard streams and, on Windows, no console window.
pub fn command(exe: &std::path::Path) -> Command {
    let mut c = Command::new(exe);
    c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(target_os = "macos")]
    {
        // An app started from Finder has a minimal PATH; Claude Code needs node and git on it.
        let path = std::env::var("PATH").unwrap_or_default();
        c.env("PATH", format!("/opt/homebrew/bin:/usr/local/bin:{path}"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// A user message in the stream-json input format, as one line.
pub fn user_message(text: &str) -> String {
    let msg = serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": text },
        "parent_tool_use_id": null,
        "session_id": "",
    });
    format!("{msg}\n")
}
