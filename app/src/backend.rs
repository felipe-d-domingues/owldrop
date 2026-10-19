//! Everything outside the window: the USB dongle (usbipd), the WSL side
//! (installed from this repo's wsl/ folder) and the bridge process that speaks
//! JSON lines with it.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub const DISTRO: &str = "Ubuntu";
const HWID: &str = "0cf3:9271";
const NO_WINDOW: u32 = 0x0800_0000;

/// What the backend tells the window.
pub enum Msg {
    Event(Value),
    /// Files handed over by another launch (Explorer's "Send with OwlDrop").
    SendFiles(Vec<PathBuf>),
    /// The WSL side went away; the backend is reconnecting.
    Disconnected,
    Setup(String),
    Fatal(String),
}

pub fn quiet(cmd: &str) -> Command {
    let mut c = Command::new(cmd);
    c.creation_flags(NO_WINDOW);
    c
}

fn usbipd() -> PathBuf {
    let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    Path::new(&pf).join("usbipd-win").join("usbipd.exe")
}

/// C:\x\y -> /mnt/c/x/y
pub fn to_wsl(p: &Path) -> Option<String> {
    let s = p.to_str()?.replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s);
    let (drive, rest) = s.split_once(':')?;
    (drive.len() == 1).then(|| format!("/mnt/{}{}", drive.to_ascii_lowercase(), rest))
}

/// /mnt/c/x/y -> C:\x\y; anything else is a path inside WSL.
pub fn to_windows(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("/mnt/") {
        let mut it = rest.splitn(2, '/');
        if let (Some(d), Some(r)) = (it.next(), it.next()) {
            if d.len() == 1 {
                return PathBuf::from(format!("{}:\\{}", d.to_ascii_uppercase(), r.replace('/', "\\")));
            }
        }
    }
    PathBuf::from(format!(r"\\wsl.localhost\{DISTRO}{}", p.replace('/', "\\")))
}

/// The repo holding wsl/: $AIRDROP_REPO, or an ancestor of the executable.
pub fn repo_root() -> Option<PathBuf> {
    if let Ok(r) = std::env::var("AIRDROP_REPO") {
        return Some(PathBuf::from(r));
    }
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find(|a| a.join("wsl").join("airdrop-bridge").exists()).map(Path::to_path_buf)
}

pub fn wsl_user() -> String {
    quiet("wsl")
        .args(["-d", DISTRO, "-e", "id", "-un"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "root".into())
}

/// Attach the AR9271 to WSL now. Err explains what the person has to do.
fn attach() -> Result<(), String> {
    let exe = usbipd();
    if !exe.exists() {
        return Err("usbipd-win isn't installed (winget install dorssel.usbipd-win).".into());
    }
    let list = quiet(exe.to_str().unwrap()).arg("list").output().map_err(|e| e.to_string())?;
    let list = String::from_utf8_lossy(&list.stdout);
    let Some(line) = list.lines().find(|l| l.contains(HWID)) else {
        return Err("The AR9271 Wi-Fi dongle isn't plugged in.".into());
    };
    if line.contains("Not shared") {
        return Err(format!(
            "The dongle isn't shared with WSL. Run once as administrator: usbipd bind --hardware-id {HWID}"
        ));
    }
    if line.contains("Attached") {
        return Ok(());
    }
    let out = quiet(exe.to_str().unwrap())
        .args(["attach", "--wsl", DISTRO, "--hardware-id", HWID])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// Copy the WSL scripts in place and patch omdrop's receiver. Idempotent.
fn install(repo: &Path, ble_exe: &Path) -> Result<(), String> {
    let (Some(r), Some(b)) = (to_wsl(repo), to_wsl(ble_exe)) else {
        return Err("the app must live on a local drive".into());
    };
    let script = format!(
        "tr -d '\\r' < '{r}/wsl/airdrop-wsl' > /tmp/airdrop-wsl && bash /tmp/airdrop-wsl install '{r}' '{b}'"
    );
    let out = quiet("wsl")
        .args(["-d", DISTRO, "-u", "root", "-e", "bash", "-c", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
    }
}

pub struct Backend {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    children: Arc<Mutex<Vec<Child>>>,
    stopping: Arc<std::sync::atomic::AtomicBool>,
}

static USBIPD_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn is_usbipd(c: &Child) -> bool {
    c.id() == USBIPD_PID.load(std::sync::atomic::Ordering::Relaxed)
}

impl Backend {
    /// Starts setup in the background; messages arrive on `tx`, and `wake`
    /// asks the window to repaint.
    /// Starts the backend in the background; messages arrive on `tx`, and
    /// `wake` asks the window to repaint. When the WSL side goes away (a WSL
    /// update or `wsl --shutdown` restarts the VM, taking the radio with it)
    /// it reconnects: re-attaches the dongle and starts a new bridge.
    pub fn start(tx: Sender<Msg>, wake: impl Fn() + Send + Sync + 'static) -> Backend {
        let stdin: Arc<Mutex<Option<ChildStdin>>> = Arc::new(Mutex::new(None));
        let children: Arc<Mutex<Vec<Child>>> = Arc::new(Mutex::new(Vec::new()));
        let stopping = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (s2, c2, stop2) = (stdin.clone(), children.clone(), stopping.clone());
        std::thread::spawn(move || {
            let send = |m: Msg| {
                let _ = tx.send(m);
                wake();
            };
            let Some(repo) = repo_root() else {
                return send(Msg::Fatal("Can't find the project folder (wsl/airdrop-bridge). Set AIRDROP_REPO.".into()));
            };
            let ble = std::env::current_exe().unwrap().with_file_name("owldrop-ble.exe");
            let mut first = true;
            while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                if !first {
                    send(Msg::Setup("WSL restarted. Reconnecting…".into()));
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
                first = false;
                send(Msg::Setup("Installing the Linux side…".into()));
                if let Err(e) = install(&repo, &ble) {
                    send(Msg::Fatal(format!("Setting up WSL failed: {e}")));
                    std::thread::sleep(std::time::Duration::from_secs(15));
                    continue;
                }
                // Wait for the dongle rather than giving up: it may be unplugged.
                send(Msg::Setup("Attaching the Wi-Fi dongle…".into()));
                let mut waited = 0;
                while let Err(e) = attach() {
                    if waited == 0 || waited % 10 == 0 {
                        send(Msg::Setup(e));
                    }
                    waited += 1;
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    if stop2.load(std::sync::atomic::Ordering::Relaxed) {
                        return;
                    }
                }
                {
                    // Drop exited children; (re)start usbipd's auto-attach,
                    // which re-attaches after a replug, if it isn't running.
                    let mut kids = c2.lock().unwrap();
                    kids.retain_mut(|c| c.try_wait().ok().flatten().is_none());
                    if !kids.iter().any(is_usbipd) {
                        if let Ok(c) = quiet(usbipd().to_str().unwrap())
                            .args(["attach", "--wsl", DISTRO, "--hardware-id", HWID, "--auto-attach"])
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn()
                        {
                            USBIPD_PID.store(c.id(), std::sync::atomic::Ordering::Relaxed);
                            kids.push(c);
                        }
                    }
                }
                send(Msg::Setup("Starting…".into()));
                let child = quiet("wsl")
                    .args(["-d", DISTRO, "-u", "root", "-e", "/usr/local/sbin/airdrop-bridge"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn();
                let mut child = match child {
                    Ok(c) => c,
                    Err(e) => {
                        send(Msg::Fatal(format!("Couldn't start WSL: {e}")));
                        std::thread::sleep(std::time::Duration::from_secs(15));
                        continue;
                    }
                };
                *s2.lock().unwrap() = child.stdin.take();
                let stdout = child.stdout.take().unwrap();
                c2.lock().unwrap().push(child);
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Ok(v) = serde_json::from_str::<Value>(&line) {
                        send(Msg::Event(v));
                    }
                }
                s2.lock().unwrap().take();
                if !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                    send(Msg::Disconnected);
                }
            }
        });
        Backend { stdin, children, stopping }
    }

    pub fn send(&self, cmd: Value) {
        if let Some(stdin) = self.stdin.lock().unwrap().as_mut() {
            let _ = writeln!(stdin, "{cmd}");
            let _ = stdin.flush();
        }
    }

    pub fn on(&self, secs: u64, name: &str, outdir: &Path, user: &str) {
        self.send(json!({"cmd": "on", "secs": secs, "name": name,
                         "outdir": to_wsl(outdir), "user": user}));
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        // Closing the bridge's stdin makes it turn the receiver off and exit; give
        // it time to, then stop usbipd's auto-attach.
        self.stopping.store(true, std::sync::atomic::Ordering::Relaxed);
        self.stdin.lock().unwrap().take();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut children = self.children.lock().unwrap();
        while std::time::Instant::now() < deadline
            && children.iter_mut().any(|c| c.try_wait().ok().flatten().is_none() && !is_usbipd(c))
        {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        for c in children.iter_mut() {
            let _ = c.kill();
        }
    }
}
