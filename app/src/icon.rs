// The app icon, drawn in code so the build needs no image files: a blue disc
// with three rings open at the bottom and a centre dot — a nearby-share mark.
// Shared by build.rs (the .exe's icon resource) and the window icon.

/// RGBA pixels, `size` x `size`, 4x supersampled.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let n = (size * SS) as f32;
    let c = n / 2.0;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for py in 0..size {
        for px in 0..size {
            let (mut rgb, mut covered) = ([0f32; 3], 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px * SS + sx) as f32 + 0.5 - c;
                    let y = (py * SS + sy) as f32 + 0.5 - c;
                    let r = (x * x + y * y).sqrt() / c;
                    if r > 0.97 {
                        continue;
                    }
                    covered += 1;
                    let t = (y + c) / n;
                    let mut col = [90.0 + (0.0 - 90.0) * t, 200.0 + (110.0 - 200.0) * t, 250.0 + (240.0 - 250.0) * t];
                    // Angle away from straight down (y grows downward).
                    let down = (y.atan2(x) - std::f32::consts::FRAC_PI_2).abs();
                    let ring = [0.24f32, 0.45, 0.66].iter().any(|rr| (r - rr).abs() < 0.06) && down > 0.62;
                    if ring || r < 0.10 {
                        col = [255.0, 255.0, 255.0];
                    }
                    for i in 0..3 {
                        rgb[i] += col[i];
                    }
                }
            }
            let i = ((py * size + px) * 4) as usize;
            if covered > 0 {
                for k in 0..3 {
                    out[i + k] = (rgb[k] / covered as f32) as u8;
                }
                out[i + 3] = (covered * 255 / (SS * SS)) as u8;
            }
        }
    }
    out
}
