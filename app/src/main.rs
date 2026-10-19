//! OwlDrop: make this PC visible to Apple devices over AirDrop, approve what
//! they send, send files to them (from this window or from Explorer's
//! right-click menu), and keep a history of what arrived.
//!
//! The radio and the AirDrop protocol run in WSL (OWL + omdrop, see wsl/);
//! this window talks to them through backend::Backend.
#![windows_subsystem = "windows"]

mod backend;
mod history;
mod icon;
mod ipc;
mod shell;
mod ui;

use backend::{to_windows, Backend, Msg};
use eframe::egui::{self, Align, Layout, RichText, Vec2};
use history::{Entry, History};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use ui::Palette;

const ASK_WAIT: f64 = 55.0; // the receiver declines unanswered asks after this

#[derive(Clone, Copy, PartialEq)]
enum Window {
    TenMinutes,
    OneHour,
    Always,
}

impl Window {
    fn secs(self) -> u64 {
        match self {
            Window::TenMinutes => 600,
            Window::OneHour => 3600,
            Window::Always => 0,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Window::TenMinutes => "10 min",
            Window::OneHour => "1 hour",
            Window::Always => "Always",
        }
    }
}

struct Ask {
    id: String,
    sender: String,
    model: String,
    files: Vec<(String, u64)>,
    links: Vec<String>,
    since: f64,
}

#[derive(Clone)]
struct Peer {
    addr: String,
    name: Option<String>,
    /// Why there's no name: "contacts" (Contacts Only), "asleep", "tls".
    note: Option<String>,
}

/// The device picker: one native window listing nearby devices.
struct Picker {
    files: Vec<PathBuf>,
    sending_to: Option<String>,
    send_started: Option<Instant>,
    last_peer: Option<Peer>,
    result: Option<(bool, String, Instant)>,
}

struct App {
    backend: Backend,
    rx: Receiver<Msg>,
    setup: Option<String>,
    fatal: Option<String>,
    user: String,
    exe: PathBuf,
    name: String,
    outdir: PathBuf,
    window: Window,
    status: Value,
    switching: Option<Instant>,
    asks: Vec<Ask>,
    history: History,
    peers: BTreeMap<String, Peer>,
    scanning: bool,
    picker: Option<Picker>,
    send_log: Vec<String>,
    last_error: Option<String>,
    explorer: bool,
    show_settings: bool,
    /// Started from Explorer: the picker is what matters, so tuck the main window away.
    minimize_once: bool,
    /// What the person last asked for, restored after WSL restarts.
    want_visible: Option<Window>,
    /// Whether the AR9271 driver is built for the running WSL kernel.
    driver: Option<bool>,
    kernel: String,
    rebuilding: Option<String>,
}

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64()
}

pub fn size(n: u64) -> String {
    let n = n as f64;
    match n {
        n if n < 1024.0 => format!("{n} B"),
        n if n < 1048576.0 => format!("{:.0} KB", n / 1024.0),
        n if n < 1073741824.0 => format!("{:.1} MB", n / 1048576.0),
        n => format!("{:.2} GB", n / 1073741824.0),
    }
}

fn toast(title: &str, body: &str) {
    use tauri_winrt_notification::Toast;
    let _ = Toast::new(Toast::POWERSHELL_APP_ID).title(title).text1(body).show();
}

fn open(p: &Path) {
    let _ = backend::quiet("explorer").arg(p).spawn();
}

fn reveal(p: &Path) {
    let _ = backend::quiet("explorer").arg(format!("/select,{}", p.display())).spawn();
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

impl App {
    fn new(cc: &eframe::CreationContext, listener: std::net::TcpListener, send: Vec<PathBuf>) -> App {
        ui::setup(&cc.egui_ctx);
        let (tx, rx) = channel();
        let c1 = cc.egui_ctx.clone();
        let backend = Backend::start(tx.clone(), move || c1.request_repaint());
        let c2 = cc.egui_ctx.clone();
        ipc::serve(listener, tx, move || c2.request_repaint());
        let history = History::load();
        let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows PC".into());
        let mut name = history.settings.name.clone().unwrap_or_else(|| {
            let lower = host.to_lowercase();
            let mut c = lower.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or(host)
        });
        name.truncate(40);
        let outdir = history.settings.outdir.clone().unwrap_or_else(|| {
            PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join("Downloads").join("OwlDrop")
        });
        let exe = std::env::current_exe().unwrap_or_default();
        // Keep Explorer's entries pointing at this executable.
        let explorer = shell::is_registered(&exe) || shell::register(&exe).is_ok();
        let mut app = App {
            backend,
            rx,
            setup: Some("Starting…".into()),
            fatal: None,
            user: backend::wsl_user(),
            exe,
            name,
            outdir,
            window: Window::TenMinutes,
            status: json!({}),
            switching: None,
            asks: Vec::new(),
            history,
            peers: BTreeMap::new(),
            scanning: false,
            picker: None,
            send_log: Vec::new(),
            last_error: None,
            explorer,
            show_settings: false,
            // The picker is drawn by the main window's frame (an immediate
            // viewport): a minimized main window stops drawing it, which froze
            // the picker mid-send. So the main window stays open behind it.
            minimize_once: false,
            want_visible: None,
            driver: None,
            kernel: String::new(),
            rebuilding: None,
        };
        if !send.is_empty() {
            app.open_picker(send);
        }
        app
    }

    fn visible(&self) -> bool {
        self.status["visible"].as_bool().unwrap_or(false) && self.status["receiving"].as_bool().unwrap_or(false)
    }

    fn busy(&self) -> bool {
        self.setup.is_some() || self.switching.is_some()
    }

    fn turn_on(&mut self, window: Window) {
        if self.driver == Some(false) {
            return;
        }
        self.want_visible = Some(window);
        self.window = window;
        self.switching = Some(Instant::now());
        self.last_error = None;
        let _ = std::fs::create_dir_all(&self.outdir);
        self.backend.on(window.secs(), &self.name, &self.outdir, &self.user);
    }

    fn turn_off(&mut self) {
        self.want_visible = None;
        self.switching = Some(Instant::now());
        self.backend.send(json!({"cmd": "off"}));
    }

    fn scan(&mut self) {
        if self.setup.is_some() {
            return; // the bridge isn't up yet; the picker retries
        }
        if !self.visible() && self.switching.is_none() {
            // The radio is what hears nearby devices.
            self.turn_on(Window::TenMinutes);
        }
        self.scanning = true;
        // AWDL addresses change every session: start the list afresh.
        self.peers.clear();
        self.backend.send(json!({"cmd": "peers", "name": self.name, "seconds": 45}));
    }

    fn open_picker(&mut self, files: Vec<PathBuf>) {
        let files: Vec<PathBuf> = files.into_iter().filter(|p| p.is_file()).collect();
        match &mut self.picker {
            Some(p) if p.sending_to.is_none() => {
                for f in files {
                    if !p.files.contains(&f) {
                        p.files.push(f);
                    }
                }
                p.result = None;
            }
            Some(_) => {} // a send is running; ignore until it finishes
            None => {
                self.picker = Some(Picker { files, sending_to: None, send_started: None, last_peer: None, result: None });
                self.scan();
            }
        }
    }

    fn send_to(&mut self, peer: &Peer) {
        let Some(p) = &mut self.picker else { return };
        let files: Vec<String> = p.files.iter().filter_map(|f| backend::to_wsl(f)).collect();
        if files.is_empty() {
            return;
        }
        p.sending_to = Some(peer.name.clone().unwrap_or_else(|| peer.addr.clone()));
        p.last_peer = Some(peer.clone());
        p.send_started = Some(Instant::now());
        p.result = None;
        self.send_log.clear();
        self.backend.send(json!({"cmd": "stop_scan"}));
        self.backend.send(json!({"cmd": "send", "to": peer.addr, "name": self.name, "files": files}));
    }

    fn handle(&mut self, ctx: &egui::Context, v: Value) {
        match v["ev"].as_str().unwrap_or("") {
            "hello" => {
                self.setup = None;
                self.fatal = None;
                self.driver = v["driver"].as_bool();
                self.kernel = v["kernel"].as_str().unwrap_or_default().to_string();
                self.backend.send(json!({"cmd": "status"}));
                if self.driver == Some(false) {
                    return;
                }
                // Back after a WSL restart (or a driver rebuild): carry on.
                if let Some(w) = self.want_visible {
                    if !self.visible() {
                        self.turn_on(w);
                    }
                }
                if self.picker.is_some() {
                    self.scan();
                }
            }
            "status" => {
                // A window that ran out (or a radio that stopped) ends the wish.
                let reason = v["reason"].as_str().unwrap_or("");
                if !v["visible"].as_bool().unwrap_or(false) && !reason.is_empty() && self.switching.is_none() {
                    self.want_visible = None;
                }
                self.status = v;
                self.switching = None;
            }
            "rebuild_step" => self.rebuilding = v["text"].as_str().map(String::from),
            "rebuild_done" => {
                self.rebuilding = None;
                if !v["ok"].as_bool().unwrap_or(false) {
                    self.last_error = Some(format!("Driver build failed: {}", v["message"].as_str().unwrap_or("")));
                }
            }
            "ask" => {
                let files = v["files"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|f| (f["name"].as_str().unwrap_or("?").to_string(), f["size"].as_u64().unwrap_or(0)))
                            .collect()
                    })
                    .unwrap_or_default();
                let ask = Ask {
                    id: v["id"].as_str().unwrap_or_default().to_string(),
                    sender: v["sender"].as_str().unwrap_or("Someone").to_string(),
                    model: v["model"].as_str().unwrap_or("").to_string(),
                    files,
                    links: v["links"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|l| l.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    since: v["time"].as_f64().unwrap_or_else(now),
                };
                let what = match (ask.files.len(), ask.links.is_empty()) {
                    (1, _) => ask.files[0].0.clone(),
                    (0, false) => "a link".into(),
                    (n, _) => format!("{n} files"),
                };
                toast(&format!("{} wants to send you a file", ask.sender), &format!("{what} · open OwlDrop to accept"));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Critical));
                self.asks.push(ask);
            }
            "ask_closed" => {
                let id = v["id"].as_str().unwrap_or_default();
                self.asks.retain(|a| a.id != id);
            }
            "received" => {
                let sender = v["sender"]["name"].as_str().unwrap_or("Unknown").to_string();
                let files: Vec<(PathBuf, u64)> = v["files"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|f| (to_windows(f["path"].as_str().unwrap_or("")), f["size"].as_u64().unwrap_or(0)))
                            .collect()
                    })
                    .unwrap_or_default();
                let url = v["url"].as_str().map(String::from);
                for (path, bytes) in &files {
                    self.history.add(Entry::received(&sender, path, *bytes, url.clone()));
                }
                let body = match (&url, files.len()) {
                    (Some(u), _) => u.clone(),
                    (None, 1) => file_name(&files[0].0),
                    (None, n) => format!("{n} files"),
                };
                toast(&format!("OwlDrop from {sender}"), &format!("Saved {body}"));
            }
            "failed" => {
                let sender = v["sender"]["name"].as_str().unwrap_or("Unknown").to_string();
                let reason = v["reason"].as_str().unwrap_or("failed").to_string();
                self.history.add(Entry::failed(&sender, &reason));
                toast("OwlDrop failed", &format!("From {sender}: {reason}"));
            }
            "peer" => {
                let p = Peer {
                    addr: v["addr"].as_str().unwrap_or_default().to_string(),
                    name: v["name"].as_str().map(String::from),
                    note: v["note"].as_str().map(String::from),
                };
                // A name, once heard, sticks for this scan.
                let keep_name = self.peers.get(&p.addr).and_then(|o| o.name.clone());
                self.peers.insert(p.addr.clone(), Peer { name: p.name.or(keep_name), ..p });
            }
            "peers_done" => {
                self.scanning = false;
                // While the picker is open and idle, keep looking.
                if self.picker.as_ref().is_some_and(|p| p.sending_to.is_none()) {
                    self.scan();
                }
            }
            "send_log" => {
                self.send_log.push(v["text"].as_str().unwrap_or_default().to_string());
                if self.send_log.len() > 200 {
                    self.send_log.remove(0);
                }
            }
            "send_done" => {
                let ok = v["ok"].as_bool().unwrap_or(false);
                let msg = v["message"].as_str().unwrap_or_default().to_string();
                if let Some(p) = &mut self.picker {
                    let who = p.sending_to.take().unwrap_or_default();
                    for f in &p.files {
                        self.history.add(Entry::sent(&who, f, ok, &msg));
                    }
                    let why = if ok {
                        format!("Sent to {who}")
                    } else if msg.contains("never opened") || msg.contains("not listening") {
                        format!("{who} didn't wake up. On it, open the share sheet (tap Share on any photo), then try again.")
                    } else if msg == "cancelled" {
                        "Cancelled.".to_string()
                    } else if msg.contains("declined") || msg.contains("403") {
                        format!("{who} declined.")
                    } else {
                        msg.clone()
                    };
                    p.result = Some((ok, why, Instant::now()));
                    if ok {
                        toast("OwlDrop", &format!("Sent to {who}"));
                    }
                }
            }
            "error" => {
                self.switching = None;
                if v["code"].as_i64() == Some(7) {
                    self.driver = Some(false);
                } else {
                    self.last_error = v["message"].as_str().map(String::from);
                }
            }
            _ => {}
        }
    }

    fn decide(&mut self, id: &str, accept: bool) {
        self.backend.send(json!({"cmd": "decide", "id": id, "accept": accept}));
        self.asks.retain(|a| a.id != id);
    }

    // ------------------------------------------------------------------ views

    fn header(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui.horizontal(|ui| {
            ui::app_icon(ui, 44.0);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("OwlDrop").size(24.0).family(ui::semibold()).color(pal.text));
                let sub = if let Some(f) = &self.fatal {
                    RichText::new(f.as_str()).color(pal.danger)
                } else if let Some(s) = &self.setup {
                    RichText::new(s.as_str()).color(pal.muted)
                } else if self.visible() {
                    RichText::new(format!("Visible as “{}”", self.name)).color(pal.ok)
                } else {
                    RichText::new("Hidden from nearby devices").color(pal.muted)
                };
                ui.label(sub.size(13.0));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::icon_button(ui, "⚙", "Settings").clicked() {
                    self.show_settings = !self.show_settings;
                }
                ui.add_space(4.0);
                let mut on = self.visible();
                let r = ui.add_enabled(
                    self.fatal.is_none() && !self.busy() && self.driver != Some(false),
                    ui::Toggle::new(&mut on, pal),
                );
                if r.changed() {
                    if on { self.turn_on(self.window) } else { self.turn_off() }
                }
                if self.busy() && self.fatal.is_none() {
                    ui.spinner();
                }
            });
        });
    }

    fn visibility(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui::card(ui, pal, |ui| {
            ui.horizontal(|ui| {
                ui::section_title(ui, pal, "Receiving");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let enabled = !self.busy() && !self.visible();
                    ui.add_enabled_ui(enabled, |ui| {
                        for w in [Window::Always, Window::OneHour, Window::TenMinutes] {
                            if ui::segment(ui, pal, w.label(), self.window == w).clicked() {
                                self.window = w;
                            }
                        }
                    });
                    ui.label(RichText::new("Stay visible").color(pal.muted).size(12.5));
                });
            });
            ui.add_space(4.0);
            let st = &self.status;
            ui.horizontal_wrapped(|ui| {
                if self.visible() {
                    ui::chip(ui, pal, pal.ok, "Everyone");
                    if let Some(r) = st["remaining"].as_u64() {
                        ui::chip(ui, pal, pal.muted, &format!("{}:{:02} left", r / 60, r % 60));
                    } else {
                        ui::chip(ui, pal, pal.muted, "Until turned off");
                    }
                    let n = st["awdl_peers"].as_u64().unwrap_or(0);
                    ui::chip(ui, pal, pal.accent, &format!("{n} Apple device{} nearby", if n == 1 { "" } else { "s" }));
                } else {
                    ui.label(
                        RichText::new("Turn on the switch so iPhones, iPads and Macs nearby can see this PC.")
                            .color(pal.muted),
                    );
                }
            });
            if let Some(e) = &self.last_error {
                ui.colored_label(pal.danger, e);
            }
        });
    }

    fn asks(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let mut decisions = Vec::new();
        for ask in &self.asks {
            let left = (ASK_WAIT - (now() - ask.since)).max(0.0);
            ui::accent_card(ui, pal, |ui| {
                ui.horizontal(|ui| {
                    ui::avatar(ui, pal, &ask.sender, 44.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&ask.sender).size(16.0).family(ui::semibold()).color(pal.text));
                        let what = if ask.files.len() == 1 {
                            "wants to share a file".to_string()
                        } else if !ask.links.is_empty() {
                            "wants to share a link".into()
                        } else {
                            format!("wants to share {} files", ask.files.len())
                        };
                        let model = if ask.model.is_empty() { String::new() } else { format!(" · {}", ask.model) };
                        ui.label(RichText::new(format!("{what}{model}")).color(pal.muted));
                    });
                });
                ui.add_space(4.0);
                for (n, s) in ask.files.iter().take(6) {
                    ui::file_row(ui, pal, n, &size(*s));
                }
                if ask.files.len() > 6 {
                    ui.label(RichText::new(format!("and {} more", ask.files.len() - 6)).color(pal.muted));
                }
                for l in &ask.links {
                    ui::file_row(ui, pal, l, "link");
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui::primary_button(ui, pal, "Accept").clicked() {
                        decisions.push((ask.id.clone(), true));
                    }
                    if ui::secondary_button(ui, pal, "Decline").clicked() {
                        decisions.push((ask.id.clone(), false));
                    }
                    ui.add_space(8.0);
                    ui::countdown(ui, pal, (left / ASK_WAIT) as f32, &format!("{left:.0}s"));
                });
            });
            ui.add_space(10.0);
        }
        for (id, yes) in decisions {
            self.decide(&id, yes);
        }
    }

    fn send_card(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui::card(ui, pal, |ui| {
            ui::section_title(ui, pal, "Send");
            ui.add_space(2.0);
            if ui::drop_zone(ui, pal, "Drop files here, or click to choose", "Then pick a nearby device").clicked() {
                if let Some(fs) = rfd::FileDialog::new().pick_files() {
                    self.open_picker(fs);
                }
            }
            if !self.explorer {
                ui.label(RichText::new("Explorer's “Send with OwlDrop” isn't installed; see Settings.").color(pal.muted).size(12.0));
            } else {
                ui.label(
                    RichText::new("Tip: right-click files in Explorer → Show more options → Send with OwlDrop.")
                        .color(pal.muted)
                        .size(12.0),
                );
            }
        });
    }

    fn history_card(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui::card(ui, pal, |ui| {
            ui.horizontal(|ui| {
                ui::section_title(ui, pal, "History");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !self.history.entries.is_empty() && ui::link_button(ui, pal, "Clear").clicked() {
                        self.history.entries.clear();
                        self.history.save();
                    }
                    if ui::link_button(ui, pal, "Open folder").clicked() {
                        let _ = std::fs::create_dir_all(&self.outdir);
                        open(&self.outdir);
                    }
                });
            });
            if self.history.entries.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Files you receive and send will show up here.").color(pal.muted));
                ui.add_space(6.0);
            }
            let mut first = true;
            for e in self.history.entries.iter().rev().take(200) {
                if !first {
                    ui::hairline(ui, pal);
                }
                first = false;
                ui::history_row(ui, pal, e, |action| match action {
                    ui::RowAction::Open(p) => open(p),
                    ui::RowAction::Reveal(p) => reveal(p),
                    ui::RowAction::Url(u) => {
                        if u.starts_with("https://") || u.starts_with("http://") {
                            open(Path::new(u))
                        }
                    }
                });
            }
        });
    }

    fn settings(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui::card(ui, pal, |ui| {
            ui::section_title(ui, pal, "Settings");
            egui::Grid::new("settings").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
                ui.label(RichText::new("Name").color(pal.muted));
                let r = ui.add_enabled(!self.visible(), egui::TextEdit::singleline(&mut self.name).desired_width(220.0));
                if r.lost_focus() {
                    self.history.settings.name = Some(self.name.clone());
                    self.history.save();
                }
                ui.end_row();
                ui.label(RichText::new("Save to").color(pal.muted));
                if ui::secondary_button(ui, pal, &self.outdir.display().to_string()).clicked() {
                    if let Some(d) = rfd::FileDialog::new().set_directory(&self.outdir).pick_folder() {
                        self.outdir = d;
                        self.history.settings.outdir = Some(self.outdir.clone());
                        self.history.save();
                    }
                }
                ui.end_row();
                ui.label(RichText::new("Explorer").color(pal.muted));
                ui.horizontal(|ui| {
                    if self.explorer {
                        ui::chip(ui, pal, pal.ok, "Send with OwlDrop installed");
                        if ui::link_button(ui, pal, "Remove").clicked() {
                            shell::unregister();
                            self.explorer = false;
                        }
                    } else if ui::secondary_button(ui, pal, "Add “Send with OwlDrop”").clicked() {
                        self.explorer = shell::register(&self.exe).is_ok();
                    }
                });
                ui.end_row();
            });
            if !self.visible() {
                ui.label(RichText::new("Name and folder apply the next time you turn OwlDrop on.").color(pal.muted).size(12.0));
            }
        });
    }

    fn driver_card(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui::accent_card(ui, pal, |ui| {
            ui::section_title(ui, pal, "Wi-Fi driver needs rebuilding");
            ui.label(
                RichText::new(format!(
                    "WSL was updated to a new Linux kernel ({}), and the AR9271 driver is built for one kernel at a time.                      Rebuilding takes about 15 minutes and runs in the background.",
                    self.kernel
                ))
                .color(pal.muted),
            );
            ui.add_space(6.0);
            if let Some(step) = &self.rebuilding {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(step).color(pal.text));
                });
            } else if ui::primary_button(ui, pal, "Rebuild driver").clicked() {
                if let Some(repo) = backend::repo_root().and_then(|r| backend::to_wsl(&r)) {
                    self.rebuilding = Some("Starting…".into());
                    self.last_error = None;
                    self.backend.send(json!({"cmd": "rebuild", "repo": repo}));
                }
            }
            if let Some(e) = &self.last_error {
                ui.colored_label(pal.danger, e);
            }
        });
    }

    /// The device picker, in a window of its own.
    fn picker_window(&mut self, ctx: &egui::Context) {
        if self.picker.is_none() {
            return;
        }
        let id = egui::ViewportId::from_hash_of("picker");
        let builder = egui::ViewportBuilder::default()
            .with_title("Send with OwlDrop")
            .with_inner_size([400.0, 470.0])
            .with_min_inner_size([360.0, 380.0])
            .with_always_on_top()
            .with_icon(std::sync::Arc::new(ui::window_icon()));
        let mut close = false;
        let mut send = None;
        let mut rescan = false;
        let mut cancel = false;
        ctx.show_viewport_immediate(id, builder, |ctx, _class| {
            let pal = Palette::of(ctx);
            if ctx.input(|i| i.viewport().close_requested()) {
                close = true;
            }
            egui::CentralPanel::default().frame(ui::window_frame(&pal)).show(ctx, |ui| {
                let p = self.picker.as_ref().unwrap();
                ui.horizontal(|ui| {
                    ui::app_icon(ui, 30.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Send with OwlDrop").size(17.0).family(ui::semibold()).color(pal.text));
                        let total: u64 = p.files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
                        let what = if p.files.len() == 1 { file_name(&p.files[0]) } else { format!("{} files", p.files.len()) };
                        ui.label(RichText::new(format!("{what} · {}", size(total))).color(pal.muted).size(12.5));
                    });
                });
                ui.add_space(10.0);

                if let Some((ok, msg, _)) = &p.result {
                    ui::result_banner(ui, &pal, *ok, msg);
                    if !*ok {
                        if let Some(peer) = &p.last_peer {
                            if ui::primary_button(ui, &pal, "Try again").clicked() {
                                send = Some(peer.clone());
                            }
                        }
                    }
                    ui.add_space(8.0);
                }
                if let Some(who) = &p.sending_to {
                    ui::accent_card(ui, &pal, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.vertical(|ui| {
                                ui.label(RichText::new(format!("Sending to {who}…")).color(pal.text));
                                ui.label(RichText::new("Waiting for them to accept").color(pal.muted).size(12.0));
                                ui.label(
                                    RichText::new("Nothing on the iPhone? Unlock it and open its share sheet to wake AirDrop.")
                                        .color(pal.muted)
                                        .size(12.0),
                                );
                            });
                        });
                        if let Some(l) = self.send_log.last() {
                            ui.add(egui::Label::new(RichText::new(l).color(pal.muted).size(11.0)).truncate());
                        }
                        ui.add_space(4.0);
                        if ui::secondary_button(ui, &pal, "Cancel").clicked() {
                            cancel = true;
                        }
                    });
                    return;
                }

                ui.horizontal(|ui| {
                    ui.label(RichText::new("NEARBY").size(11.5).color(pal.muted).family(ui::semibold()));
                    if self.scanning || self.setup.is_some() {
                        ui.add(egui::Spinner::new().size(12.0));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !self.scanning && ui::link_button(ui, &pal, "Look again").clicked() {
                            rescan = true;
                        }
                    });
                });
                ui.add_space(4.0);
                let mut peers: Vec<Peer> = self.peers.values().cloned().collect();
                peers.sort_by_key(|p| (p.name.is_none(), p.note.clone()));
                if peers.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui::radar(ui, &pal, 70.0);
                        ui.add_space(10.0);
                        ui.label(RichText::new("Looking for devices…").color(pal.text));
                        ui.label(
                            RichText::new("On the iPhone: unlock it and set AirDrop to “Everyone for 10 Minutes”.")
                                .color(pal.muted)
                                .size(12.0),
                        );
                    });
                } else {
                    egui::ScrollArea::vertical().auto_shrink([false, true]).max_height(ui.available_height() - 40.0).show(ui, |ui| {
                        for peer in &peers {
                            let (title, sub, enabled) = match (&peer.name, peer.note.as_deref()) {
                                (Some(n), _) => (n.clone(), "Ready".to_string(), true),
                                (None, Some("contacts")) => (
                                    "Apple device".into(),
                                    "Contacts Only: switch it to “Everyone for 10 Minutes”".into(),
                                    false,
                                ),
                                _ => ("Apple device".into(), "Not answering: unlock it and open AirDrop".into(), false),
                            };
                            if ui::device_row(ui, &pal, &title, &sub, enabled).clicked() && enabled {
                                send = Some(peer.clone());
                            }
                        }
                    });
                }
                ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
                    if ui::secondary_button(ui, &pal, "Close").clicked() {
                        close = true;
                    }
                });
            });
        });
        if let Some(peer) = send {
            self.send_to(&peer);
        }
        // Cancel, or give up on a send that never reported back (the sender
        // itself stops within ~3 minutes: 60 s wake, Ask, Upload).
        let stale = self
            .picker
            .as_ref()
            .and_then(|p| p.send_started.filter(|_| p.sending_to.is_some()))
            .is_some_and(|t| t.elapsed() > Duration::from_secs(240));
        if cancel || stale {
            self.backend.send(json!({"cmd": "cancel_send"}));
            if let Some(p) = &mut self.picker {
                p.sending_to = None;
                p.result = Some((false, if cancel { "Cancelled.".into() } else { "No answer from the device.".into() }, Instant::now()));
            }
        }
        if rescan {
            self.scan();
        }
        // Close a while after a successful send.
        if let Some(Picker { result: Some((true, _, at)), .. }) = &self.picker {
            if at.elapsed() > Duration::from_secs(3) {
                close = true;
            } else {
                ctx.request_repaint_after(Duration::from_millis(300));
            }
        }
        if close && self.picker.as_ref().is_some_and(|p| p.sending_to.is_none()) {
            self.picker = None;
            self.scanning = false;
            self.backend.send(json!({"cmd": "stop_scan"}));
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if std::mem::take(&mut self.minimize_once) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Event(v) => self.handle(ctx, v),
                Msg::Setup(s) => {
                    self.setup = Some(s);
                    self.fatal = None;
                }
                Msg::Disconnected => {
                    if let Some(p) = &mut self.picker {
                        if p.sending_to.take().is_some() {
                            p.result = Some((false, "The connection to WSL dropped during the send.".into(), Instant::now()));
                        }
                    }
                    // Keep want_visible: the backend reconnects and `hello` restores it.
                    self.status = json!({});
                    self.scanning = false;
                    self.rebuilding = None;
                    self.asks.clear();
                }
                Msg::Fatal(s) => {
                    self.fatal = Some(s);
                    self.setup = None;
                }
                Msg::SendFiles(files) => {
                    if files.is_empty() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    } else {
                        self.open_picker(files);
                    }
                }
            }
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            self.open_picker(dropped);
        }
        if !self.asks.is_empty() || self.switching.is_some() || self.scanning || self.picker.is_some() || self.visible() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        let pal = Palette::of(ctx);
        egui::CentralPanel::default().frame(ui::window_frame(&pal)).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                self.header(ui, &pal);
                ui.add_space(14.0);
                if self.fatal.is_none() {
                    if self.driver == Some(false) || self.rebuilding.is_some() {
                        self.driver_card(ui, &pal);
                        ui.add_space(10.0);
                    }
                    self.asks(ui, &pal);
                    if self.show_settings {
                        self.settings(ui, &pal);
                        ui.add_space(10.0);
                    }
                    self.visibility(ui, &pal);
                    ui.add_space(10.0);
                    self.send_card(ui, &pal);
                    ui.add_space(10.0);
                }
                self.history_card(ui, &pal);
                ui.add_space(Vec2::splat(8.0).y);
            });
        });
        self.picker_window(ctx);
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let send: Vec<PathBuf> = match args.first().map(String::as_str) {
        Some("--send") => args[1..].iter().map(PathBuf::from).collect(),
        _ => Vec::new(),
    };
    // A second launch hands its files to the running app and leaves.
    let listener = match ipc::claim() {
        Ok(l) => l,
        Err(_) => {
            if ipc::forward(&send) {
                return Ok(());
            }
            // Port taken by something else: run anyway, on a throwaway listener.
            std::net::TcpListener::bind("127.0.0.1:0").expect("loopback")
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OwlDrop")
            .with_icon(std::sync::Arc::new(ui::window_icon()))
            .with_inner_size([520.0, 760.0])
            .with_min_inner_size([420.0, 480.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("OwlDrop", options, Box::new(move |cc| Ok(Box::new(App::new(cc, listener, send)))))
}
