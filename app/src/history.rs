//! Transfer history and settings, kept in %LOCALAPPDATA%\OwlDrop\history.json.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    pub time: i64,
    /// "in" or "out"
    pub direction: String,
    pub ok: bool,
    pub peer: String,
    pub title: String,
    pub path: Option<PathBuf>,
    pub url: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    pub name: Option<String>,
    pub outdir: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct History {
    pub entries: Vec<Entry>,
    pub settings: Settings,
}

const KEEP: usize = 1000;

fn file() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    base.join("OwlDrop").join("history.json")
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

impl Entry {
    fn new(direction: &str, ok: bool, peer: &str, title: String) -> Entry {
        Entry {
            time: chrono::Local::now().timestamp(),
            direction: direction.into(),
            ok,
            peer: peer.into(),
            title,
            path: None,
            url: None,
        }
    }

    pub fn received(peer: &str, path: &Path, bytes: u64, url: Option<String>) -> Entry {
        let title = match &url {
            Some(u) => u.clone(),
            None => format!("{}  ({})", name_of(path), crate::size(bytes)),
        };
        Entry { path: Some(path.to_path_buf()), url, ..Entry::new("in", true, peer, title) }
    }

    pub fn failed(peer: &str, reason: &str) -> Entry {
        Entry::new("in", false, peer, format!("Failed: {reason}"))
    }

    pub fn sent(peer: &str, path: &Path, ok: bool, msg: &str) -> Entry {
        let title = if ok { name_of(path) } else { format!("{}: {msg}", name_of(path)) };
        Entry { path: Some(path.to_path_buf()), ..Entry::new("out", ok, peer, title) }
    }

    pub fn when(&self) -> String {
        use chrono::TimeZone;
        chrono::Local
            .timestamp_opt(self.time, 0)
            .single()
            .map(|t| t.format("%d %b %H:%M").to_string())
            .unwrap_or_default()
    }
}

impl History {
    pub fn load() -> History {
        std::fs::read(file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let f = file();
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            let tmp = f.with_extension("tmp");
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(tmp, f);
            }
        }
    }

    pub fn add(&mut self, e: Entry) {
        self.entries.push(e);
        if self.entries.len() > KEEP {
            let n = self.entries.len() - KEEP;
            self.entries.drain(..n);
        }
        self.save();
    }
}
