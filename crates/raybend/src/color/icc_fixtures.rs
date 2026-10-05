//! Synthetic ICC bytes, written independently of LCMS/tag extraction. Fixtures
//! deliberately keep conflicting sRGB matrix tags to detect ignored CLUT stages.
use super::icc::{IccRole, RgbIcc, srgb_icc};
#[derive(Clone, Copy, Debug)]
pub(crate) enum LutKind {
    Lut8,
    Lut16,
    Mab,
}
pub(crate) fn profile(version: u8, kind: LutKind, lab: bool) -> RgbIcc {
    let mut bytes = srgb_icc().unwrap().bytes().to_vec();
    bytes[8] = version;
    bytes[9..12].fill(0);
    bytes[84..100].fill(0);
    bytes[20..24].copy_from_slice(if lab { b"Lab " } else { b"XYZ " });
    for (name, output) in [(b"A2B0", false), (b"B2A0", true)] {
        let tag = lut(kind, output);
        add_tag(&mut bytes, name, &tag);
    }
    RgbIcc::parse(&bytes, IccRole::Display).unwrap()
}
fn add_tag(bytes: &mut Vec<u8>, name: &[u8; 4], tag: &[u8]) {
    let count = u32::from_be_bytes(bytes[128..132].try_into().unwrap()) as usize;
    let end = 132 + count * 12;
    bytes.splice(end..end, [0_u8; 12]);
    bytes[128..132].copy_from_slice(&((count + 1) as u32).to_be_bytes());
    for i in 0..count {
        let at = 136 + i * 12;
        let offset = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) + 12;
        bytes[at..at + 4].copy_from_slice(&offset.to_be_bytes());
    }
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    let offset = bytes.len() as u32;
    bytes[end..end + 4].copy_from_slice(name);
    bytes[end + 4..end + 8].copy_from_slice(&offset.to_be_bytes());
    bytes[end + 8..end + 12].copy_from_slice(&(tag.len() as u32).to_be_bytes());
    bytes.extend_from_slice(tag);
    let length = bytes.len() as u32;
    bytes[..4].copy_from_slice(&length.to_be_bytes());
}
fn lut(kind: LutKind, output: bool) -> Vec<u8> {
    const GRID: u8 = 5;
    let values = || {
        let mut values = Vec::new();
        for r in 0..GRID {
            for g in 0..GRID {
                for b in 0..GRID {
                    let [r, g, b] = [r, g, b].map(|v| f32::from(v) / f32::from(GRID - 1));
                    // Smooth cross-channel coupling; not separable into matrix/TRC.
                    let rgb = if output {
                        [
                            0.02 + 0.65 * r + 0.2 * g * b,
                            0.01 + 0.7 * g + 0.15 * r * b,
                            0.03 + 0.6 * b + 0.25 * r * g,
                        ]
                    } else {
                        [
                            0.45 * r + 0.02 * g * b,
                            0.48 * g + 0.01 * r * b,
                            0.4 * b + 0.03 * r * g,
                        ]
                    };
                    values.extend(rgb);
                }
            }
        }
        values
    };
    let mut tag = Vec::new();
    match kind {
        LutKind::Lut8 | LutKind::Lut16 => {
            tag.extend_from_slice(if matches!(kind, LutKind::Lut8) {
                b"mft1\0\0\0\0"
            } else {
                b"mft2\0\0\0\0"
            });
            tag.extend([3, 3, GRID, 0]);
            for i in 0..9 {
                tag.extend((if i % 4 == 0 { 65536_i32 } else { 0 }).to_be_bytes());
            }
            if matches!(kind, LutKind::Lut16) {
                tag.extend(2_u16.to_be_bytes());
                tag.extend(2_u16.to_be_bytes());
                for _ in 0..3 {
                    tag.extend([0, 0, 255, 255]);
                }
                for v in values() {
                    tag.extend(((v * 65535.0).round() as u16).to_be_bytes());
                }
                for _ in 0..3 {
                    tag.extend([0, 0, 255, 255]);
                }
            } else {
                for _ in 0..3 {
                    tag.extend(0_u8..=255);
                }
                for v in values() {
                    tag.push((v * 255.0).round() as u8);
                }
                for _ in 0..3 {
                    tag.extend(0_u8..=255);
                }
            }
        }
        LutKind::Mab => {
            tag.extend_from_slice(if output {
                b"mBA \0\0\0\0"
            } else {
                b"mAB \0\0\0\0"
            });
            tag.extend([3, 3, 0, 0]);
            // B curves, no matrix/M curves, CLUT, A curves. Both curve sets identity.
            for offset in [32_u32, 0, 0, 104, 68] {
                tag.extend(offset.to_be_bytes());
            }
            for _ in 0..6 {
                tag.extend_from_slice(b"curv\0\0\0\0\0\0\0\0");
            }
            tag.extend([GRID, GRID, GRID]);
            tag.extend([0_u8; 13]);
            tag.extend([2, 0, 0, 0]);
            for v in values() {
                tag.extend(((v * 65535.0).round() as u16).to_be_bytes());
            }
        }
    }
    tag
}
pub(crate) fn sharp_profile() -> RgbIcc {
    let mut bytes = srgb_icc().unwrap().bytes().to_vec();
    let mut tag = lut(LutKind::Lut16, true);
    tag[50..52].copy_from_slice(&4096_u16.to_be_bytes());
    tag.truncate(tag.len() - 12);
    for _ in 0..3 {
        for i in 0..4096 {
            tag.extend((if i >= 2048 { 65535_u16 } else { 0 }).to_be_bytes());
        }
    }
    add_tag(&mut bytes, b"B2A0", &tag);
    RgbIcc::parse(&bytes, IccRole::Display).unwrap()
}
