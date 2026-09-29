//! 小图读取：JPEG 只读头部段；TIFF 家族 RAW 只读有界头部和指向的 JPEG。
//! RAW 的调用方必须在隔离 worker 内。没有可用内嵌图返回 None，由调用方决定兜底。
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const HEAD_LIMIT: u64 = 256 * 1024;
const JPEG_LIMIT: usize = 16 * 1024 * 1024;

pub(crate) struct EmbeddedJpeg {
    pub bytes: Vec<u8>,
    pub orientation: Option<u16>,
}

pub(crate) fn read(path: &Path) -> Option<EmbeddedJpeg> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut magic = [0; 2];
    file.read_exact(&mut magic).ok()?;
    if magic == [0xff, 0xd8] {
        return read_jpeg(&mut file);
    }
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut head = Vec::new();
    file.by_ref().take(HEAD_LIMIT).read_to_end(&mut head).ok()?;
    let orientation = super::tiff::parse(&head)
        .and_then(|info| info.orientation)
        .map(|value| super::meta::normalize_orientation(Some(value)));
    for (offset, size) in super::tiff::embedded_jpeg_spans(&head) {
        if !(4..=JPEG_LIMIT).contains(&size) || offset.checked_add(size as u64)? > len { continue; }
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut bytes = vec![0; size];
        file.read_exact(&mut bytes).ok()?;
        if bytes.starts_with(&[0xff, 0xd8]) {
            return Some(EmbeddedJpeg { bytes, orientation });
        }
    }
    None
}

fn read_jpeg(reader: &mut (impl Read + Seek)) -> Option<EmbeddedJpeg> {
    // 已消费 SOI。遇到 SOS 即止，绝不为了查缩略图去读 JPEG 主图扫描数据。
    for _ in 0..256 {
        let mut marker = [0; 2];
        reader.read_exact(&mut marker).ok()?;
        if marker[0] != 0xff { return None; }
        while marker[1] == 0xff { reader.read_exact(&mut marker[1..]).ok()?; }
        if matches!(marker[1], 0xda | 0xd9) { return None; }
        if matches!(marker[1], 0x01 | 0xd0..=0xd7) { continue; }
        let mut length = [0; 2];
        reader.read_exact(&mut length).ok()?;
        let size = usize::from(u16::from_be_bytes(length)).checked_sub(2)?;
        if marker[1] != 0xe1 {
            reader.seek(SeekFrom::Current(size as i64)).ok()?;
            continue;
        }
        let mut segment = vec![0; size];
        reader.read_exact(&mut segment).ok()?;
        let Some(tiff) = segment.strip_prefix(b"Exif\0\0") else { continue };
        let orientation = super::tiff::parse(tiff)
            .and_then(|info| info.orientation)
            .map(|value| super::meta::normalize_orientation(Some(value)));
        for (offset, size) in super::tiff::embedded_jpeg_spans(tiff) {
            let offset = usize::try_from(offset).ok()?;
            let Some(bytes) = tiff.get(offset..offset.checked_add(size)?) else { continue };
            if bytes.starts_with(&[0xff, 0xd8]) {
                return Some(EmbeddedJpeg { bytes: bytes.to_vec(), orientation });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn tiff_with_thumb(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        let mut b = b"II\x2a\0\x08\0\0\0".to_vec();
        b.extend(1u16.to_le_bytes());
        b.extend(0x112u16.to_le_bytes()); b.extend(3u16.to_le_bytes());
        b.extend(1u32.to_le_bytes()); b.extend(u32::from(orientation).to_le_bytes());
        b.extend(26u32.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        for (tag, value) in [(0x201u16, 56u32), (0x202, jpeg.len() as u32)] {
            b.extend(tag.to_le_bytes()); b.extend(4u16.to_le_bytes());
            b.extend(1u32.to_le_bytes()); b.extend(value.to_le_bytes());
        }
        b.extend(0u32.to_le_bytes()); b.extend(jpeg); b
    }
    #[test]
    fn jpeg_exif_thumbnail_and_orientation_without_reading_main_scan() {
        let thumb = [0xff, 0xd8, 1, 2, 0xff, 0xd9];
        let tiff = tiff_with_thumb(&thumb, 6);
        let mut jpeg = vec![0xff, 0xe1];
        jpeg.extend(((tiff.len() + 8) as u16).to_be_bytes());
        jpeg.extend(b"Exif\0\0"); jpeg.extend(tiff);
        let end = jpeg.len() as u64;
        jpeg.extend([0xff, 0xda, 0, 2]);
        let mut reader = std::io::Cursor::new(jpeg);
        let found = read_jpeg(&mut reader).unwrap();
        assert_eq!(found.bytes, thumb); assert_eq!(found.orientation, Some(6));
        assert_eq!(reader.position(), end);
    }
    #[test]
    fn raw_span_is_bounded_and_works_with_unicode_paths() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("竖片.NEF");
        let bytes = tiff_with_thumb(&[0xff, 0xd8, 0xff, 0xd9], 8);
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(read(&path).unwrap().orientation, Some(8));
        for len in 0..bytes.len() { std::fs::write(&path, &bytes[..len]).unwrap(); assert!(read(&path).is_none()); }
        let mut broken = bytes;
        broken[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
        std::fs::write(&path, broken).unwrap(); assert!(read(&path).is_none());
    }
    #[test]
    fn small_jpeg_renders_embedded_pixels_and_orientation_without_a_valid_main_image() {
        use crate::thumbnail::render::{render_file, SizeClass};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("小缩略图.JPG");
        let mut thumb = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut thumb)
            .encode_image(&image::RgbImage::from_pixel(24, 12, image::Rgb([90, 60, 30]))).unwrap();
        for orientation in 1..=8 {
            let tiff = tiff_with_thumb(&thumb, orientation);
            let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe1];
            jpeg.extend(((tiff.len() + 8) as u16).to_be_bytes());
            jpeg.extend(b"Exif\0\0"); jpeg.extend(tiff);
            // 故意没有主图，完整 JPEG 解码一定失败。
            jpeg.extend([0xff, 0xd9]);
            std::fs::write(&path, jpeg).unwrap();
            let rendered = render_file(&path, SizeClass::Grid).unwrap().unwrap();
            assert_eq!((rendered.width, rendered.height), if orientation >= 5 { (12, 24) } else { (24, 12) });
        }
    }
    #[test]
    fn tiny_embedded_raw_never_needs_sensor_or_camera_metadata() {
        use crate::raw::{RawBackend, DecodeRequest, RawlerBackend, PixelSource};
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("测试.NEF");
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode_image(&image::RgbImage::from_pixel(24, 12, image::Rgb([90, 60, 30]))).unwrap();
        std::fs::write(&path, tiff_with_thumb(&jpeg, 6)).unwrap();
        // 无厂商/机型/传感器数据；旧的完整 RAW 路径无法打开这个合成文件。
        let decoded = RawlerBackend::new().decode(&DecodeRequest::embedded(&path, 384)).unwrap();
        assert_eq!((decoded.width, decoded.height), (24, 12));
        assert_eq!(decoded.orientation, Some(6));
        assert_eq!(decoded.source, PixelSource::EmbeddedPreview);
    }
    #[test]
    fn missing_exif_and_invalid_segment_never_scan_pixels() {
        for bytes in [vec![], vec![0xff, 0xda], vec![0xff, 0xe1, 0, 1], vec![1, 2]] {
            assert!(read_jpeg(&mut std::io::Cursor::new(bytes)).is_none());
        }
    }
}
