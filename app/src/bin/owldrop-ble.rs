//! Broadcasts a BLE advertisement carrying Apple (0x004C) manufacturer data
//! from Windows' own Bluetooth stack.
//!
//! WSL has no Bluetooth, so the sender's wake advert (omdrop-awdl's
//! ble-airdrop-adv.py, which talks to BlueZ) is replaced inside WSL by a shim
//! that runs this through WSL interop. Like the original it prints a line with
//! START once the radio is advertising and STOP at the end; exit 3 if the
//! adapter refuses.
//!
//!   owldrop-ble.exe ble-adv HEX [SECONDS]     HEX = the value after the company id

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementPublisher, BluetoothLEAdvertisementPublisherStatus as Status,
    BluetoothLEManufacturerData,
};
use windows::Storage::Streams::DataWriter;

const APPLE: u16 = 0x004C;

fn stamp(msg: &str) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    println!("{:.3} {}", now.as_secs_f64(), msg);
}

fn hex(s: &str) -> Option<Vec<u8>> {
    (s.len() % 2 == 0)
        .then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect())
        .flatten()
}

fn run(value: &[u8], seconds: f64) -> windows::core::Result<i32> {
    let md = BluetoothLEManufacturerData::new()?;
    md.SetCompanyId(APPLE)?;
    let w = DataWriter::new()?;
    w.WriteBytes(value)?;
    md.SetData(&w.DetachBuffer()?)?;
    let publisher = BluetoothLEAdvertisementPublisher::new()?;
    publisher.Advertisement()?.ManufacturerData()?.Append(&md)?;
    publisher.Start()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(publisher.Status()?, Status::Created | Status::Waiting) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    if publisher.Status()? != Status::Started {
        stamp(&format!("REGISTER FAILED status={:?}", publisher.Status()?));
        return Ok(3);
    }
    let hexed: String = value.iter().map(|b| format!("{b:02x}")).collect();
    stamp(&format!("START windows-ble mfr=4c00 value={hexed}"));
    let end = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < end && publisher.Status()? == Status::Started {
        std::thread::sleep(Duration::from_millis(200));
    }
    publisher.Stop()?;
    stamp("STOP advertisement unregistered");
    Ok(0)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Accept both `owldrop-ble HEX SECS` and `owldrop-ble ble-adv HEX SECS`.
    let rest = if args.get(1).map(String::as_str) == Some("ble-adv") { &args[2..] } else { &args[1..] };
    let Some(value) = rest.first().and_then(|h| hex(h)) else {
        eprintln!("usage: owldrop-ble ble-adv HEX [SECONDS]");
        std::process::exit(2);
    };
    let seconds = rest.get(1).and_then(|s| s.parse().ok()).unwrap_or(30.0);
    match run(&value, seconds) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            stamp(&format!("REGISTER FAILED {e}"));
            std::process::exit(3);
        }
    }
}
