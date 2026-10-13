//! Debug aid: with `KOOPAGIT_SNAPSHOT=<file.png>` set, a dialog renders itself
//! to that PNG shortly after opening and closes. Used to review layouts
//! without screen-capture access.

use slint::ComponentHandle;

/// Arms the snapshot timer for `component` if `KOOPAGIT_SNAPSHOT` is set.
pub fn arm<C: ComponentHandle + 'static>(component: &C) {
    let Some(path) = std::env::var_os("KOOPAGIT_SNAPSHOT") else {
        return;
    };
    let weak = component.as_weak();
    slint::Timer::single_shot(std::time::Duration::from_millis(800), move || {
        let Some(c) = weak.upgrade() else { return };
        match c.window().take_snapshot() {
            Ok(buf) => {
                let png = encode_png(buf.width(), buf.height(), buf.as_bytes());
                if let Err(e) = std::fs::write(&path, png) {
                    eprintln!("snapshot: {e}");
                }
            }
            Err(e) => eprintln!("snapshot: {e}"),
        }
        let _ = c.hide();
        let _ = slint::quit_event_loop();
    });
}

/// Minimal PNG encoder (RGBA8, stored/uncompressed deflate blocks).
pub(crate) fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width as usize * 4 + 1) * height as usize);
    for row in rgba.chunks(width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        z.extend(len.to_le_bytes());
        z.extend((!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend(((b << 16) | a).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend(width.to_be_bytes());
    ihdr.extend(height.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend(crc.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc_known_value() {
        assert_eq!(super::crc32(b"IEND"), 0xAE42_6082);
    }
}
