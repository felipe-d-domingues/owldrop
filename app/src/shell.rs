//! Explorer integration, for the current user only (HKCU, no admin):
//!  - "Send with OwlDrop" on every file's right-click menu (on Windows 11,
//!    under "Show more options"),
//!  - "OwlDrop" in the Send to submenu.
//! Both run `owldrop.exe --send <files>`.

use crate::backend::quiet;
use std::path::{Path, PathBuf};

const KEY: &str = r"HKCU\Software\Classes\*\shell\OwlDrop";

fn reg(args: &[&str]) -> bool {
    quiet("reg").args(args).output().map(|o| o.status.success()).unwrap_or(false)
}

fn sendto_link() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(Path::new(&appdata).join(r"Microsoft\Windows\SendTo\OwlDrop.lnk"))
}

pub fn register(exe: &Path) -> Result<(), String> {
    let exe_s = exe.display().to_string();
    let command = format!("\"{exe_s}\" --send \"%1\"");
    let icon = format!("\"{exe_s}\",0");
    let ok = reg(&["add", KEY, "/ve", "/d", "Send with OwlDrop", "/f"])
        && reg(&["add", KEY, "/v", "Icon", "/d", &icon, "/f"])
        // One launch per selected file; ipc gathers them into one picker.
        && reg(&["add", KEY, "/v", "MultiSelectModel", "/d", "Player", "/f"])
        && reg(&["add", &format!(r"{KEY}\command"), "/ve", "/d", &command, "/f"]);
    if !ok {
        return Err("couldn't write the context-menu entry to the registry".into());
    }
    let link = sendto_link().ok_or("no %APPDATA%")?;
    let ps = format!(
        "$s = New-Object -ComObject WScript.Shell; $l = $s.CreateShortcut('{}'); $l.TargetPath = '{}'; \
         $l.Arguments = '--send'; $l.IconLocation = '{},0'; $l.Description = 'Send with OwlDrop'; $l.Save()",
        link.display(),
        exe_s,
        exe_s
    );
    let out = quiet("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &ps]).output();
    match out {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err("couldn't create the Send to shortcut".into()),
    }
}

pub fn unregister() {
    reg(&["delete", KEY, "/f"]);
    if let Some(l) = sendto_link() {
        let _ = std::fs::remove_file(l);
    }
}

pub fn is_registered(exe: &Path) -> bool {
    let out = quiet("reg").args(["query", &format!(r"{KEY}\command"), "/ve"]).output();
    let cmd_ok = out
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&exe.display().to_string()))
        .unwrap_or(false);
    cmd_ok && sendto_link().is_some_and(|l| l.exists())
}
