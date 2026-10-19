//! One app at a time. The first instance listens on a loopback port; a later
//! launch (Explorer's "Send with OwlDrop", which starts us once per selected
//! file) hands its files to it and exits, so they all land in one picker.

use crate::backend::Msg;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

const PORT: u16 = 47391;

fn addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, PORT))
}

/// Ok(listener) if we are the first instance.
pub fn claim() -> std::io::Result<TcpListener> {
    TcpListener::bind(addr())
}

/// Give `files` to the running instance (an empty list just brings it forward).
pub fn forward(files: &[PathBuf]) -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(&addr(), Duration::from_millis(800)) else { return false };
    let paths: Vec<String> = files.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    writeln!(s, "{}", serde_json::to_string(&paths).unwrap()).is_ok()
}

pub fn serve(listener: TcpListener, tx: Sender<Msg>, wake: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            if BufReader::new(stream).read_line(&mut line).is_ok() {
                if let Ok(paths) = serde_json::from_str::<Vec<String>>(&line) {
                    let _ = tx.send(Msg::SendFiles(paths.into_iter().map(PathBuf::from).collect()));
                    wake();
                }
            }
        }
    });
}
