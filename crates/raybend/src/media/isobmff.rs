//! Bounded adapters for containers emitted by our AVIF encoder.
use crate::{Error, Result};
type Boxes = Vec<([u8; 4], Vec<u8>)>;
fn error() -> Error {
    Error::Unsupported("AVIF 容器结构无效或不受支持".into())
}

fn serialize(boxes: &Boxes) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for (kind, data) in boxes {
        out.extend(bx(kind, data)?);
    }
    Ok(out)
}
pub(crate) fn shift_iloc(payload: &mut [u8], delta: u32) -> Result<()> {
    if payload.get(..6) != Some(&[0, 0, 0, 0, 0x44, 0]) || payload.len() < 8 {
        return Err(error());
    }
    let count = u16::from_be_bytes(payload[6..8].try_into().unwrap());
    let mut at = 8;
    for _ in 0..count {
        let head = payload.get(at..at + 6).ok_or_else(error)?;
        if head[2..4] != [0, 0] {
            return Err(error());
        }
        let extents = u16::from_be_bytes(head[4..6].try_into().unwrap());
        at += 6;
        for _ in 0..extents {
            let extent = payload.get_mut(at..at + 8).ok_or_else(error)?;
            let value = u32::from_be_bytes(extent[..4].try_into().unwrap())
                .checked_add(delta)
                .ok_or_else(error)?;
            extent[..4].copy_from_slice(&value.to_be_bytes());
            at += 8;
        }
    }
    if at != payload.len() {
        return Err(error());
    }
    Ok(())
}

/// ICC attaches to the primary decoded RGB item; AV1 payload and YCbCr matrix
/// remain identical. Only our encoder's bounded v0 container schema is accepted.
pub(crate) fn attach_icc(bytes: &[u8], icc: &[u8]) -> Result<Vec<u8>> {
    if icc.is_empty() || icc.len() > 16 * 1024 * 1024 {
        return Err(error());
    }
    let mut top = boxes(bytes)?;
    let meta = top
        .iter_mut()
        .find(|(kind, _)| kind == b"meta")
        .ok_or_else(error)?;
    if meta.1.get(..4) != Some(&[0, 0, 0, 0]) {
        return Err(error());
    }
    let old_len = meta.1.len();
    let mut children = boxes(&meta.1[4..])?;
    let primary = &children
        .iter()
        .find(|(kind, _)| kind == b"pitm")
        .ok_or_else(error)?
        .1;
    if primary.len() != 6 || primary[..4] != [0, 0, 0, 0] {
        return Err(error());
    }
    let primary = u16::from_be_bytes(primary[4..6].try_into().unwrap());
    let iprp = &mut children
        .iter_mut()
        .find(|(kind, _)| kind == b"iprp")
        .ok_or_else(error)?
        .1;
    let mut properties = boxes(iprp)?;
    let ipco = &mut properties
        .iter_mut()
        .find(|(kind, _)| kind == b"ipco")
        .ok_or_else(error)?
        .1;
    let mut items = boxes(ipco)?;
    if items.iter().any(|(kind, data)| {
        kind == b"colr" && (data.starts_with(b"prof") || data.starts_with(b"rICC"))
    }) {
        return Err(error());
    }
    let index = u8::try_from(items.len() + 1).map_err(|_| error())?;
    if index > 127 {
        return Err(error());
    }
    items.push((*b"colr", [b"prof".as_slice(), icc].concat()));
    *ipco = serialize(&items)?;
    let ipma = &mut properties
        .iter_mut()
        .find(|(kind, _)| kind == b"ipma")
        .ok_or_else(error)?
        .1;
    if ipma.len() < 8 || ipma[..4] != [0, 0, 0, 0] {
        return Err(error());
    }
    let count = u32::from_be_bytes(ipma[4..8].try_into().unwrap());
    if count > 256 {
        return Err(error());
    }
    let mut at = 8;
    let mut found = false;
    let mut changed = ipma[..8].to_vec();
    for _ in 0..count {
        let head = ipma.get(at..at + 3).ok_or_else(error)?;
        let item = u16::from_be_bytes(head[..2].try_into().unwrap());
        let n = usize::from(head[2]);
        let refs = ipma.get(at + 3..at + 3 + n).ok_or_else(error)?;
        changed.extend(&head[..2]);
        if item == primary {
            if found {
                return Err(error());
            }
            found = true;
            changed.push(head[2].checked_add(1).ok_or_else(error)?);
            changed.extend(refs);
            changed.push(index);
        } else {
            changed.push(head[2]);
            changed.extend(refs);
        }
        at += 3 + n;
    }
    if !found || at != ipma.len() {
        return Err(error());
    }
    *ipma = changed;
    *iprp = serialize(&properties)?;
    let mut result = vec![0; 4];
    result.extend(serialize(&children)?);
    let delta =
        u32::try_from(result.len().checked_sub(old_len).ok_or_else(error)?).map_err(|_| error())?;
    for (kind, data) in &mut children {
        if kind == b"iloc" {
            shift_iloc(data, delta)?;
        }
    }
    meta.1 = vec![0; 4];
    meta.1.extend(serialize(&children)?);
    serialize(&top)
}
pub(crate) fn bx(kind: &[u8; 4], data: &[u8]) -> Result<Vec<u8>> {
    let len = u32::try_from(data.len() + 8).map_err(|_| error())?;
    Ok([len.to_be_bytes().to_vec(), kind.to_vec(), data.to_vec()].concat())
}
pub(crate) fn boxes(data: &[u8]) -> Result<Vec<([u8; 4], Vec<u8>)>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        if at + 8 > data.len() {
            return Err(error());
        }
        let n = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
        if n < 8 || at + n > data.len() {
            return Err(error());
        }
        out.push((
            data[at + 4..at + 8].try_into().unwrap(),
            data[at + 8..at + n].to_vec(),
        ));
        at += n;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attaching_icc_keeps_av1_bytes_decoded_pixels_and_xmp_offsets() {
        use image::ImageEncoder;
        let mut original = Vec::new();
        image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut original, 10, 90)
            .with_num_threads(Some(1))
            .write_image(
                &[80, 130, 210].repeat(6),
                3,
                2,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let profile = crate::color::icc::srgb_icc().unwrap();
        let marked = attach_icc(&original, profile.bytes()).unwrap();
        let mdat = |bytes: &[u8]| {
            boxes(bytes)
                .unwrap()
                .into_iter()
                .find(|(kind, _)| kind == b"mdat")
                .unwrap()
                .1
        };
        assert_eq!(mdat(&original), mdat(&marked));
        assert_eq!(
            image::load_from_memory(&original).unwrap().to_rgb8(),
            image::load_from_memory(&marked).unwrap().to_rgb8()
        );
        let decode = crate::color::input::decode_bitmap(&marked).unwrap();
        assert_eq!(decode.embedded_icc.unwrap().id(), profile.id());
        let xmp = crate::export::metadata::avif_xmp(&marked, b"<x:xmpmeta/>").unwrap();
        let decode = crate::color::input::decode_bitmap(&xmp).unwrap();
        assert_eq!(decode.embedded_icc.unwrap().id(), profile.id());
        assert_eq!(
            image::load_from_memory(&marked).unwrap().to_rgb8(),
            image::load_from_memory(&xmp).unwrap().to_rgb8()
        );
        assert!(attach_icc(&marked, profile.bytes()).is_err());
        for malformed in [vec![], vec![0; 8], original[..12].to_vec()] {
            assert!(attach_icc(&malformed, profile.bytes()).is_err());
        }
    }
    #[test]
    fn absolute_extent_offsets_are_bounded_and_checked_for_overflow() {
        let mut valid = vec![0, 0, 0, 0, 0x44, 0, 0, 1, 0, 1, 0, 0, 0, 1];
        valid.extend(100u32.to_be_bytes());
        valid.extend(8u32.to_be_bytes());
        shift_iloc(&mut valid, 30).unwrap();
        assert_eq!(&valid[14..18], 130u32.to_be_bytes());
        valid[14..18].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(shift_iloc(&mut valid, 1).is_err());
        for length in 0..valid.len() {
            assert!(shift_iloc(&mut valid[..length].to_vec(), 0).is_err());
        }
        valid[0] = 1;
        assert!(shift_iloc(&mut valid, 0).is_err());
    }
}
