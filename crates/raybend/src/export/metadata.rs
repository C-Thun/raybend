//! Native container metadata for our own encoded outputs, never rewriting source files.
use crate::{Error, Result};
use exif::{Field, In, Tag, Value};
use std::io::Cursor;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub fields: Vec<Field>,
    pub keywords: Vec<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub copyright: Option<String>,
}
impl Metadata {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let reader = exif::Reader::new();
        let parsed = reader
            .read_from_container(&mut Cursor::new(&bytes))
            .or_else(|_| crate::media::tiff::read_exif(&bytes));
        let mut value = Self::default();
        if let Ok(parsed) = parsed {
            value.copyright = parsed
                .get_field(Tag::Copyright, In::PRIMARY)
                .and_then(as_text);
            value.author = parsed.get_field(Tag::Artist, In::PRIMARY).and_then(as_text);
            value.description = parsed
                .get_field(Tag::ImageDescription, In::PRIMARY)
                .and_then(as_text);
            if let Some(field) = parsed.get_field(Tag(exif::Context::Tiff, 33723), In::PRIMARY) {
                let data = match &field.value {
                    Value::Byte(data) | Value::Undefined(data, _) => data.clone(),
                    Value::Long(words) => words
                        .iter()
                        .flat_map(|word| {
                            if parsed.little_endian() {
                                word.to_le_bytes()
                            } else {
                                word.to_be_bytes()
                            }
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                value.merge_iptc(&data)?;
            }
            // Preserve typed shooting EXIF/GPS. Only explicitly safe TIFF fields; offsets,
            // MakerNote, thumbnails and embedded image data describe the source, not the result.
            value.fields = parsed
                .fields()
                .filter(|f| {
                    f.ifd_num == In::PRIMARY
                        && ((f.tag.context() == exif::Context::Exif
                            && !matches!(
                                f.tag,
                                Tag::MakerNote
                                    | Tag::PixelXDimension
                                    | Tag::PixelYDimension
                                    | Tag::ColorSpace
                            ))
                            || f.tag.context() == exif::Context::Gps
                            || matches!(
                                f.tag,
                                Tag::Make
                                    | Tag::Model
                                    | Tag::DateTime
                                    | Tag::Artist
                                    | Tag::Copyright
                                    | Tag::ImageDescription
                                    | Tag::XResolution
                                    | Tag::YResolution
                                    | Tag::ResolutionUnit
                            ))
                })
                .filter(|f| !matches!(f.value, Value::Unknown(..)))
                .cloned()
                .collect();
        }
        if let Some(packet) = jpeg_iptc(&bytes)? {
            value.merge_iptc(&packet)?;
        }
        // Native packets are UTF-8 XML; parse namespaced fields rather than stripping
        // XML tags or relying on ASCII substring matching for Chinese text.
        if let Some(packet) = native_xmp(&bytes)? {
            value.merge_xmp(&packet)?;
        }
        Ok(value)
    }
    pub fn merge_iptc(&mut self, data: &[u8]) -> Result<()> {
        let mut at = 0;
        let mut utf8 = false;
        let mut entries = Vec::new();
        while at < data.len() && data[at] != 0 {
            if at + 5 > data.len() || data[at] != 0x1c {
                return Err(error());
            }
            let record = data[at + 1];
            let tag = data[at + 2];
            let n = u16::from_be_bytes(data[at + 3..at + 5].try_into().unwrap()) as usize;
            at += 5;
            if n & 0x8000 != 0 {
                return Err(Error::Unsupported("扩展长度 IPTC 暂不支持".into()));
            }
            let text = data.get(at..at + n).ok_or_else(error)?;
            at += n;
            if record == 1 && tag == 90 && text == b"\x1b%G" {
                utf8 = true;
            }
            if record == 2 {
                entries.push((tag, text));
            }
        }
        for (tag, text) in entries {
            let text = if utf8 {
                std::str::from_utf8(text)
                    .map_err(|e| Error::Unsupported(e.to_string()))?
                    .to_string()
            } else {
                text.iter().map(|b| char::from(*b)).collect()
            };
            match tag {
                25 => self.keywords.push(text),
                80 => self.author = Some(text),
                120 => self.description = Some(text),
                116 => self.copyright = Some(text),
                _ => {}
            }
        }
        self.keywords.sort();
        self.keywords.dedup();
        Ok(())
    }
    pub fn merge_xmp(&mut self, packet: &[u8]) -> Result<()> {
        let text = std::str::from_utf8(packet).map_err(|e| Error::Unsupported(e.to_string()))?;
        // Camera TIFF tags can include a C-string terminator in their declared length.
        // Trim only terminal NUL padding; malformed XML or embedded NULs remain errors.
        let doc = roxmltree::Document::parse(text.trim_end_matches('\0'))
            .map_err(|e| Error::Unsupported(format!("XMP 读取失败：{e}")))?;
        for node in doc.descendants().filter(|n| {
            n.is_element() && n.tag_name().namespace() == Some("http://purl.org/dc/elements/1.1/")
        }) {
            let values: Vec<String> = node
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "li")
                .filter_map(|n| n.text())
                .map(ToString::to_string)
                .collect();
            match node.tag_name().name() {
                "subject" => self.keywords.extend(values),
                "creator" => self.author = values.first().cloned().or(self.author.take()),
                "description" => {
                    self.description = values.first().cloned().or(self.description.take())
                }
                "rights" => self.copyright = values.first().cloned().or(self.copyright.take()),
                _ => {}
            }
        }
        self.keywords.sort();
        self.keywords.dedup();
        Ok(())
    }
    pub fn normalized(&self, width: u32, height: u32) -> Vec<Field> {
        let mut fields = self.fields.clone();
        let mut set = |tag, value| {
            fields.retain(|f| f.tag != tag);
            fields.push(Field {
                tag,
                ifd_num: In::PRIMARY,
                value,
            });
        };
        set(Tag::Orientation, Value::Short(vec![1]));
        set(Tag::ImageWidth, Value::Long(vec![width]));
        set(Tag::ImageLength, Value::Long(vec![height]));
        set(Tag::PixelXDimension, Value::Long(vec![width]));
        set(Tag::PixelYDimension, Value::Long(vec![height]));
        set(Tag::ColorSpace, Value::Short(vec![1]));
        for (tag, text) in [
            (Tag::Artist, &self.author),
            (Tag::Copyright, &self.copyright),
            (Tag::ImageDescription, &self.description),
        ] {
            if let Some(text) = text {
                set(tag, Value::Ascii(vec![text.as_bytes().to_vec()]));
            }
        }
        fields
    }
    pub fn exif(&self, width: u32, height: u32) -> Result<Vec<u8>> {
        write_fields(&self.normalized(width, height), None)
    }
    pub fn xmp(&self) -> Vec<u8> {
        fn list(values: impl IntoIterator<Item = String>, kind: &str) -> String {
            format!(
                "<rdf:{kind}>{}</rdf:{kind}>",
                values
                    .into_iter()
                    .map(|s| format!("<rdf:li>{}</rdf:li>", escape(&s)))
                    .collect::<String>()
            )
        }
        let keywords = list(self.keywords.clone(), "Bag");
        let author = list(self.author.clone(), "Seq");
        let description = self.description.as_deref().unwrap_or("");
        let copyright = self.copyright.as_deref().unwrap_or("");
        format!("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:subject>{keywords}</dc:subject><dc:creator>{author}</dc:creator><dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:description><dc:rights><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:rights></rdf:Description></rdf:RDF></x:xmpmeta>",escape(description),escape(copyright)).into_bytes()
    }
}
fn as_text(f: &Field) -> Option<String> {
    if let Value::Ascii(v) = &f.value {
        v.first()
            .map(|s| String::from_utf8_lossy(s).trim_end_matches('\0').into())
    } else {
        None
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn write_fields(fields: &[Field], strips: Option<&[u8]>) -> Result<Vec<u8>> {
    let mut writer = exif::experimental::Writer::new();
    for field in fields {
        writer.push_field(field);
    }
    let data = [strips.unwrap_or(&[])];
    if strips.is_some() {
        writer.set_strips(&data, In::PRIMARY);
    }
    let mut out = Cursor::new(Vec::new());
    writer
        .write(&mut out, true)
        .map_err(|e| Error::Unsupported(format!("元数据编码失败：{e}")))?;
    Ok(out.into_inner())
}
fn error() -> Error {
    Error::Unsupported("编码后的元数据容器结构无效或过大".into())
}
fn be(v: u32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}
fn bx(kind: &[u8; 4], data: &[u8]) -> Result<Vec<u8>> {
    let len = u32::try_from(data.len() + 8).map_err(|_| error())?;
    Ok([be(len), kind.to_vec(), data.to_vec()].concat())
}
fn boxes(data: &[u8]) -> Result<Vec<([u8; 4], Vec<u8>)>> {
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
/// avif-serialize emits version-0 iloc with 32-bit absolute extents. Add a standard
/// MIME XMP item and cdsc reference, shifting every original extent by meta growth.
/// This adapter accepts only that encoder's bounded schema and rejects other layouts.
pub fn avif_xmp(bytes: &[u8], xmp: &[u8]) -> Result<Vec<u8>> {
    let mut top = boxes(bytes)?;
    let old_meta = top
        .iter()
        .find(|(k, _)| k == b"meta")
        .ok_or_else(error)?
        .1
        .clone();
    if old_meta.get(..4) != Some(&[0, 0, 0, 0]) {
        return Err(error());
    }
    let mut children = boxes(&old_meta[4..])?;
    let mut new_item = vec![2, 0, 0, 0];
    new_item.extend(4u16.to_be_bytes());
    new_item.extend(0u16.to_be_bytes());
    new_item.extend(b"mime\0application/rdf+xml\0");
    let infe = bx(b"infe", &new_item)?;
    let mut offsets = Vec::new();
    for (kind, payload) in &mut children {
        if kind == b"iinf" {
            if payload.len() < 6 || payload[..4] != [0, 0, 0, 0] {
                return Err(error());
            }
            let n = u16::from_be_bytes(payload[4..6].try_into().unwrap());
            payload[4..6].copy_from_slice(&(n + 1).to_be_bytes());
            payload.extend(&infe);
        }
        if kind == b"iloc" {
            if payload.len() < 8 || payload[..6] != [0, 0, 0, 0, 0x44, 0] {
                return Err(error());
            }
            let count = u16::from_be_bytes(payload[6..8].try_into().unwrap());
            let mut at = 8;
            for _ in 0..count {
                if at + 6 > payload.len() {
                    return Err(error());
                }
                let id = u16::from_be_bytes(payload[at..at + 2].try_into().unwrap());
                if id >= 4 {
                    return Err(error());
                }
                let n = u16::from_be_bytes(payload[at + 4..at + 6].try_into().unwrap()) as usize;
                at += 6;
                for _ in 0..n {
                    if at + 8 > payload.len() {
                        return Err(error());
                    }
                    offsets.push(at);
                    at += 8;
                }
            }
            if at != payload.len() {
                return Err(error());
            }
            payload[6..8].copy_from_slice(&(count + 1).to_be_bytes());
            payload.extend(4u16.to_be_bytes());
            payload.extend(0u16.to_be_bytes());
            payload.extend(1u16.to_be_bytes());
            payload.extend(0u32.to_be_bytes());
            payload.extend(u32::try_from(xmp.len()).map_err(|_| error())?.to_be_bytes());
        }
    }
    let mut cdsc = Vec::new();
    cdsc.extend(4u16.to_be_bytes());
    cdsc.extend(1u16.to_be_bytes());
    cdsc.extend(1u16.to_be_bytes());
    let cdsc = bx(b"cdsc", &cdsc)?;
    if let Some((_, p)) = children.iter_mut().find(|(k, _)| k == b"iref") {
        if p.get(..4) != Some(&[0, 0, 0, 0]) {
            return Err(error());
        }
        p.extend(cdsc);
    } else {
        children.push((*b"iref", [vec![0; 4], cdsc].concat()));
    }
    let meta_len = 4 + children.iter().map(|(_, p)| p.len() + 8).sum::<usize>();
    let delta = u32::try_from(meta_len - old_meta.len()).map_err(|_| error())?;
    for (kind, p) in &mut children {
        if kind == b"iloc" {
            for at in &offsets {
                let v = u32::from_be_bytes(p[*at..*at + 4].try_into().unwrap())
                    .checked_add(delta)
                    .ok_or_else(error)?;
                p[*at..*at + 4].copy_from_slice(&v.to_be_bytes());
            }
            let at = p.len() - 8;
            let end = u32::try_from(bytes.len())
                .map_err(|_| error())?
                .checked_add(delta)
                .ok_or_else(error)?;
            p[at..at + 4].copy_from_slice(&end.to_be_bytes());
        }
    }
    let mut meta = vec![0; 4];
    for (k, p) in children {
        meta.extend(bx(&k, &p)?);
    }
    for (k, p) in &mut top {
        if k == b"meta" {
            *p = meta.clone();
        }
        if k == b"mdat" {
            p.extend(xmp);
        }
    }
    // Current serializer writes mdat last, so the new XMP extent points to its appended payload.
    if top.last().map(|(k, _)| k) != Some(b"mdat") {
        return Err(error());
    }
    let mut out = Vec::new();
    for (k, p) in top {
        out.extend(bx(&k, &p)?);
    }
    Ok(out)
}
fn riff_chunk(kind: &[u8; 4], data: &[u8]) -> Result<Vec<u8>> {
    let n = u32::try_from(data.len()).map_err(|_| error())?;
    let mut out = [kind.to_vec(), n.to_le_bytes().to_vec(), data.to_vec()].concat();
    if n % 2 == 1 {
        out.push(0)
    }
    Ok(out)
}
pub fn webp_metadata(bytes: &[u8], exif: &[u8], xmp: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    if bytes.len() < 12
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WEBP"
        || w == 0
        || h == 0
        || w > 1 << 24
        || h > 1 << 24
    {
        return Err(error());
    }
    let mut body = b"WEBP".to_vec();
    let mut vp8x = vec![0x0c, 0, 0, 0];
    vp8x.extend(&(w - 1).to_le_bytes()[..3]);
    vp8x.extend(&(h - 1).to_le_bytes()[..3]);
    body.extend(riff_chunk(b"VP8X", &vp8x)?);
    body.extend(&bytes[12..]);
    body.extend(riff_chunk(b"EXIF", exif)?);
    body.extend(riff_chunk(b"XMP ", xmp)?);
    Ok([
        b"RIFF".to_vec(),
        u32::try_from(body.len())
            .map_err(|_| error())?
            .to_le_bytes()
            .to_vec(),
        body,
    ]
    .concat())
}
pub fn jpeg_metadata(bytes: &[u8], xmp: &[u8], metadata: &Metadata) -> Result<Vec<u8>> {
    fn app(marker: u8, payload: &[u8]) -> Result<Vec<u8>> {
        let n = u16::try_from(payload.len() + 2)
            .map_err(|_| Error::Unsupported("JPEG 元数据超过单段 64 KiB 上限".into()))?;
        Ok([
            vec![0xff, marker],
            n.to_be_bytes().to_vec(),
            payload.to_vec(),
        ]
        .concat())
    }
    if bytes.get(..2) != Some(&[0xff, 0xd8]) {
        return Err(error());
    }
    let mut out = bytes[..2].to_vec();
    out.extend(app(
        0xe1,
        &[b"http://ns.adobe.com/xap/1.0/\0".as_slice(), xmp].concat(),
    )?);
    // IPTC UTF-8 character set + keywords/byline/caption/copyright in Photoshop IRB.
    let mut iptc = vec![0x1c, 1, 90, 0, 3, 0x1b, 0x25, 0x47];
    for (tag, text) in metadata
        .keywords
        .iter()
        .map(|s| (25, s))
        .chain(metadata.author.iter().map(|s| (80, s)))
        .chain(metadata.description.iter().map(|s| (120, s)))
        .chain(metadata.copyright.iter().map(|s| (116, s)))
    {
        let n = u16::try_from(text.len()).map_err(|_| error())?;
        iptc.extend([0x1c, 2, tag]);
        iptc.extend(n.to_be_bytes());
        iptc.extend(text.as_bytes());
    }
    let mut irb = b"Photoshop 3.0\0".to_vec();
    irb.extend(b"8BIM\x04\x04\0\0");
    irb.extend(
        u32::try_from(iptc.len())
            .map_err(|_| error())?
            .to_be_bytes(),
    );
    irb.extend(&iptc);
    if iptc.len() % 2 == 1 {
        irb.push(0)
    }
    out.extend(app(0xed, &irb)?);
    out.extend(&bytes[2..]);
    Ok(out)
}
fn crc(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for b in data {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
pub fn png_xmp(bytes: &[u8], xmp: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() < 33 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(error());
    }
    let payload = [b"XML:com.adobe.xmp\0\0\0\0\0".as_slice(), xmp].concat();
    let chunk = [b"iTXt".as_slice(), &payload].concat();
    let mut out = bytes[..33].to_vec();
    out.extend(
        u32::try_from(payload.len())
            .map_err(|_| error())?
            .to_be_bytes(),
    );
    out.extend(&chunk);
    out.extend(crc(&chunk).to_be_bytes());
    out.extend(&bytes[33..]);
    Ok(out)
}

/// Read the standard native XMP location from outputs; all offsets are bounds checked.
pub fn native_xmp(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    if bytes.starts_with(b"\xff\xd8") {
        let mut at = 2;
        while at + 4 <= bytes.len() {
            if bytes[at] != 0xff {
                return Err(error());
            }
            let marker = bytes[at + 1];
            if marker == 0xda || marker == 0xd9 {
                break;
            }
            let n = u16::from_be_bytes(bytes[at + 2..at + 4].try_into().unwrap()) as usize;
            if n < 2 || at + 2 + n > bytes.len() {
                return Err(error());
            }
            let data = &bytes[at + 4..at + 2 + n];
            let header = b"http://ns.adobe.com/xap/1.0/\0";
            if marker == 0xe1 && data.starts_with(header) {
                return Ok(Some(data[header.len()..].to_vec()));
            }
            at += 2 + n;
        }
        return Ok(None);
    }
    if bytes.starts_with(b"\x89PNG") {
        if bytes.len() < 33 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(error());
        }
        let mut at = 8;
        while at + 12 <= bytes.len() {
            let n = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            let data = bytes.get(at + 8..at + 8 + n).ok_or_else(error)?;
            if &bytes[at + 4..at + 8] == b"iTXt" && data.starts_with(b"XML:com.adobe.xmp\0") {
                let start = b"XML:com.adobe.xmp\0".len();
                let mut p = data.get(start..).ok_or_else(error)?;
                let compressed = *p.first().ok_or_else(error)?;
                if p.get(1) != Some(&0) {
                    return Err(error());
                }
                p = &p[2..];
                for _ in 0..2 {
                    let end = p.iter().position(|b| *b == 0).ok_or_else(error)?;
                    p = &p[end + 1..];
                }
                if compressed == 0 {
                    return Ok(Some(p.to_vec()));
                }
                if compressed != 1 {
                    return Err(error());
                }
                use std::io::Read;
                let mut out = Vec::new();
                flate2::read::ZlibDecoder::new(p)
                    .take(1_048_577)
                    .read_to_end(&mut out)?;
                if out.len() > 1_048_576 {
                    return Err(Error::Unsupported("XMP 超过 1 MiB 上限".into()));
                }
                return Ok(Some(out));
            }
            at = at.checked_add(n + 12).ok_or_else(error)?;
        }
        return Ok(None);
    }
    if bytes.starts_with(b"RIFF") {
        let mut at = 12;
        while at + 8 <= bytes.len() {
            let n = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
            let data = bytes.get(at + 8..at + 8 + n).ok_or_else(error)?;
            if &bytes[at..at + 4] == b"XMP " {
                return Ok(Some(data.to_vec()));
            }
            at = at.checked_add(8 + n + n % 2).ok_or_else(error)?;
        }
        return Ok(None);
    }
    if crate::media::tiff::is_tiff_family(bytes) {
        let parsed =
            crate::media::tiff::read_exif(bytes).map_err(|e| Error::Unsupported(e.to_string()))?;
        return Ok(parsed
            .get_field(Tag(exif::Context::Tiff, 700), In::PRIMARY)
            .and_then(|f| match &f.value {
                Value::Byte(data) | Value::Undefined(data, _) => Some(data.clone()),
                _ => None,
            }));
    }
    if bytes.get(4..8) == Some(b"ftyp") {
        let top = boxes(bytes)?;
        let Some((_, meta)) = top.iter().find(|(k, _)| k == b"meta") else {
            return Ok(None);
        };
        let children = boxes(meta.get(4..).ok_or_else(error)?)?;
        let Some((_, iinf)) = children.iter().find(|(k, _)| k == b"iinf") else {
            return Ok(None);
        };
        let mut id = None;
        for (k, p) in boxes(iinf.get(6..).ok_or_else(error)?)? {
            if k == *b"infe"
                && p.get(8..12) == Some(b"mime")
                && p.windows(b"application/rdf+xml".len())
                    .any(|w| w == b"application/rdf+xml")
            {
                id = Some(u16::from_be_bytes(p[4..6].try_into().unwrap()))
            }
        }
        let Some(id) = id else { return Ok(None) };
        let iloc = &children
            .iter()
            .find(|(k, _)| k == b"iloc")
            .ok_or_else(error)?
            .1;
        if iloc.get(..6) != Some(&[0, 0, 0, 0, 0x44, 0]) {
            return Err(error());
        }
        let mut at = 8;
        let count = u16::from_be_bytes(iloc[6..8].try_into().unwrap());
        for _ in 0..count {
            let head = iloc.get(at..at + 6).ok_or_else(error)?;
            let current = u16::from_be_bytes(head[..2].try_into().unwrap());
            let n = u16::from_be_bytes(head[4..6].try_into().unwrap());
            at += 6;
            for _ in 0..n {
                let extent = iloc.get(at..at + 8).ok_or_else(error)?;
                let start = u32::from_be_bytes(extent[..4].try_into().unwrap()) as usize;
                let len = u32::from_be_bytes(extent[4..8].try_into().unwrap()) as usize;
                if current == id {
                    return Ok(Some(
                        bytes
                            .get(start..start.checked_add(len).ok_or_else(error)?)
                            .ok_or_else(error)?
                            .to_vec(),
                    ));
                }
                at += 8;
            }
        }
        return Ok(None);
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_tiff_metadata_preserves_fields_xmp_and_source_bytes() {
        let source = Metadata {
            author: Some("摄影师".into()),
            keywords: vec!["旅行".into()],
            ..Default::default()
        };
        // RW2 stores XMP as Undefined with a trailing NUL included in its length.
        let packet = [source.xmp(), vec![0]].concat();
        let fields = vec![
            Field {
                tag: Tag::Make,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"Panasonic".to_vec()]),
            },
            Field {
                tag: Tag::ExposureTime,
                ifd_num: In::PRIMARY,
                value: Value::Rational(vec![exif::Rational { num: 1, denom: 200 }]),
            },
            Field {
                tag: Tag::GPSLatitudeRef,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"N".to_vec()]),
            },
            Field {
                tag: Tag(exif::Context::Tiff, 700),
                ifd_num: In::PRIMARY,
                value: Value::Undefined(packet.clone(), 0),
            },
        ];
        let temp = tempfile::tempdir().unwrap();
        for (little, magic) in [
            (true, 0x0055u16),
            (true, 0x4f52),
            (true, 0x5352),
            (false, 0x524f),
            (false, 0x5253),
        ] {
            let mut writer = exif::experimental::Writer::new();
            for field in &fields {
                writer.push_field(field);
            }
            let mut out = Cursor::new(Vec::new());
            writer.write(&mut out, little).unwrap();
            let mut bytes = out.into_inner();
            bytes[2..4].copy_from_slice(&if little {
                magic.to_le_bytes()
            } else {
                magic.to_be_bytes()
            });
            assert!(
                exif::Reader::new().read_raw(bytes.clone()).is_err(),
                "fixture must exercise non-42 RAW header"
            );
            let path = temp.path().join("原片.RAW");
            std::fs::write(&path, &bytes).unwrap();
            let metadata =
                Metadata::read(&path).expect("recognized RAW metadata must not block export");
            assert_eq!(
                std::fs::read(&path).unwrap(),
                bytes,
                "never rewrite a source header"
            );
            assert_eq!(metadata.author, source.author);
            assert_eq!(metadata.keywords, source.keywords);
            assert_eq!(native_xmp(&bytes).unwrap(), Some(packet.clone()));
            for tag in [Tag::Make, Tag::ExposureTime, Tag::GPSLatitudeRef] {
                assert!(
                    metadata.fields.iter().any(|field| field.tag == tag),
                    "missing typed field {tag}"
                );
            }
            let exported = exif::Reader::new()
                .read_raw(metadata.exif(16, 12).unwrap())
                .unwrap();
            let shutter = exported.get_field(Tag::ExposureTime, In::PRIMARY).unwrap();
            assert!(
                matches!(&shutter.value, Value::Rational(v) if v[0].num == 1 && v[0].denom == 200)
            );
        }
    }

    #[test]
    fn xmp_accepts_terminal_nuls_but_rejects_embedded_nuls_and_trailing_garbage() {
        let source = Metadata {
            author: Some("摄影师".into()),
            keywords: vec!["旅行".into()],
            ..Default::default()
        };
        for padding in [b"".as_slice(), b"\0", b"\n \0\0"] {
            let packet = [source.xmp(), padding.to_vec()].concat();
            let mut parsed = Metadata::default();
            parsed.merge_xmp(&packet).unwrap();
            assert_eq!(parsed.author, source.author);
            assert_eq!(parsed.keywords, source.keywords);
        }
        for packet in [
            b"<x>broken\0inside</x>".to_vec(),
            [source.xmp(), b"\0garbage".to_vec()].concat(),
            vec![0, 0],
        ] {
            assert!(Metadata::default().merge_xmp(&packet).is_err());
        }
    }

    #[test]
    fn tiff_xmp_checks_full_header_and_rejects_broken_known_ifd() {
        // CRW shares the byte-order prefix, but is not a TIFF IFD container.
        assert!(native_xmp(b"II\x1a\0HEAPCCDR").unwrap().is_none());
        for bytes in [
            b"II*\0".as_slice(),
            b"IIU\0\xff\xff\xff\xff",
            b"MMOR\xff\xff\xff\xff",
        ] {
            assert!(
                native_xmp(bytes).is_err(),
                "recognized but broken TIFF must remain an error"
            );
        }
    }

    #[test]
    #[ignore = "read-only source probe; set RAYBEND_EXPORT_METADATA_SOURCE explicitly"]
    fn source_metadata_smoke() {
        let path = std::env::var_os("RAYBEND_EXPORT_METADATA_SOURCE").expect("provide source path");
        let path = Path::new(&path);
        let before = std::fs::read(path).unwrap();
        let metadata = Metadata::read(path).unwrap();
        assert!(metadata.fields.iter().any(|field| field.tag == Tag::Make));
        let image = crate::display::output::Rgb16Image::from_fn(8, 6, |x, y| {
            image::Rgb([(x * 1000 + y * 97) as u16, 12345, 54321])
        });
        for format in ["avif", "webp", "jpeg", "png", "tiff"] {
            let bytes = crate::export::output::encode(&image, format, 90, &metadata).unwrap();
            let exif = exif::Reader::new()
                .read_from_container(&mut Cursor::new(&bytes))
                .unwrap();
            assert!(exif.get_field(Tag::Make, In::PRIMARY).is_some());
            println!(
                "metadata smoke: {format}, {} typed fields, {} bytes",
                metadata.fields.len(),
                bytes.len()
            );
        }
        assert_eq!(
            std::fs::read(path).unwrap(),
            before,
            "source bytes unchanged"
        );
    }

    #[test]
    fn iptc_only_source_unicode_roundtrip() {
        let md = Metadata {
            keywords: vec!["中文".into()],
            author: Some("作者".into()),
            copyright: Some("©光伴".into()),
            ..Default::default()
        };
        let bytes = jpeg_metadata(&[0xff, 0xd8, 0xff, 0xd9], &md.xmp(), &md).unwrap();
        let mut parsed = Metadata::default();
        parsed
            .merge_iptc(&jpeg_iptc(&bytes).unwrap().unwrap())
            .unwrap();
        assert_eq!(parsed.keywords, md.keywords);
        assert_eq!(parsed.author, md.author);
        assert_eq!(parsed.copyright, md.copyright);
        assert!(parsed.merge_iptc(&[0x1c, 2, 25, 0xff, 0xff]).is_err());
    }
    #[test]
    fn xml_and_container_boundaries() {
        let mut metadata = Metadata::default();
        metadata
            .merge_xmp(
                &Metadata {
                    keywords: vec!["中文 & <标签>".into()],
                    copyright: Some("© 作者".into()),
                    ..Default::default()
                }
                .xmp(),
            )
            .unwrap();
        assert_eq!(metadata.keywords, vec!["中文 & <标签>"]);
        assert!(metadata.merge_xmp(b"broken").is_err());
        for bytes in [
            b"\xff\xd8\xff\xe1\xff\xff".as_slice(),
            b"RIFF1234WEBPXMP \xff\xff\xff\xff",
            b"\x89PNG\r\n\x1a\n\xff\xff\xff\xffiTXt",
            b"\0\0\0\x04ftyp",
        ] {
            assert!(native_xmp(bytes).is_err());
        }
        assert!(native_xmp(b"no metadata").unwrap().is_none());
        assert!(avif_xmp(b"garbage", b"xml").is_err());
    }
}

fn jpeg_iptc(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    if !bytes.starts_with(b"\xff\xd8") {
        return Ok(None);
    }
    let mut at = 2;
    while at + 4 <= bytes.len() {
        let marker = bytes[at + 1];
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        let n = u16::from_be_bytes(bytes[at + 2..at + 4].try_into().unwrap()) as usize;
        if n < 2 {
            return Err(error());
        }
        let data = bytes.get(at + 4..at + 2 + n).ok_or_else(error)?;
        if marker == 0xed && data.starts_with(b"Photoshop 3.0\0") {
            let mut p = 14;
            while p + 7 <= data.len() {
                if &data[p..p + 4] != b"8BIM" {
                    return Err(error());
                }
                let id = u16::from_be_bytes(data[p + 4..p + 6].try_into().unwrap());
                p += 6;
                let len = usize::from(data[p]) + 1;
                p += len + len % 2;
                let size = data.get(p..p + 4).ok_or_else(error)?;
                let size = u32::from_be_bytes(size.try_into().unwrap()) as usize;
                p += 4;
                let packet = data.get(p..p + size).ok_or_else(error)?;
                if id == 0x0404 {
                    return Ok(Some(packet.to_vec()));
                }
                p += size + size % 2;
            }
        }
        at += 2 + n;
    }
    Ok(None)
}
