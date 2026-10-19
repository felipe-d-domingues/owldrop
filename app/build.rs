// Embeds the app icon (drawn by src/icon.rs) as the executables' icon
// resource, so Explorer, the taskbar and the context menu show it.
include!("src/icon.rs");

use std::io::Write;

/// An .ico with 32-bit BMP images (no PNG encoder needed).
fn ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&s| {
            let rgba = icon_rgba(s);
            let mut b = Vec::new();
            // BITMAPINFOHEADER; the height counts the XOR and AND masks.
            for v in [40u32, s, s * 2] {
                b.extend(v.to_le_bytes());
            }
            b.extend(1u16.to_le_bytes());
            b.extend(32u16.to_le_bytes());
            b.extend([0u8; 24]);
            for y in (0..s).rev() {
                for x in 0..s {
                    let i = ((y * s + x) * 4) as usize;
                    b.extend([rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
                }
            }
            let mask_row = ((s + 31) / 32 * 4) as usize;
            b.extend(vec![0u8; mask_row * s as usize]);
            b
        })
        .collect();
    let mut f = Vec::new();
    f.extend([0u16, 1, sizes.len() as u16].iter().flat_map(|v| v.to_le_bytes()));
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (s, img) in sizes.iter().zip(&images) {
        let d = if *s >= 256 { 0 } else { *s as u8 };
        f.extend([d, d, 0, 0]);
        f.extend(1u16.to_le_bytes());
        f.extend(32u16.to_le_bytes());
        f.extend((img.len() as u32).to_le_bytes());
        f.extend(offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        f.extend(img);
    }
    f
}

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=build.rs");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico_path = out.join("airdrop.ico");
    std::fs::write(&ico_path, ico(&[16, 24, 32, 48, 64, 256])).unwrap();
    let rc = out.join("airdrop.rc");
    let mut f = std::fs::File::create(&rc).unwrap();
    writeln!(f, "1 ICON \"{}\"", ico_path.display().to_string().replace('\\', "/")).unwrap();
    drop(f);
    embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().unwrap();
}
