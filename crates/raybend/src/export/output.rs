//! Single full-precision output engine for all export consumers.
use super::{
    Preset, VariantSnapshot,
    metadata::{self, Metadata},
};
use crate::{Error, Result};
use image::ImageEncoder;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Default)]
pub struct Naming {
    pub taken_at: Option<i64>,
    pub brand: Option<String>,
    pub model: Option<String>,
}
fn fail(text: impl Into<String>) -> Error {
    Error::Unsupported(text.into())
}
pub fn encode(
    image: &crate::display::output::Rgb16Image,
    format: &str,
    quality: u8,
    metadata: &Metadata,
) -> Result<Vec<u8>> {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return Err(fail("输出尺寸不能为空"));
    }
    if !matches!(format, "tiff" | "png") && !(1..=100).contains(&quality) {
        return Err(fail("质量应为 1–100"));
    }
    let exif = metadata.exif(w, h)?;
    let xmp = metadata.xmp();
    let mut data = Vec::new();
    let rgb = || {
        image
            .as_raw()
            .iter()
            .map(|v| ((u32::from(*v) + 128) / 257) as u8)
            .collect::<Vec<u8>>()
    };
    match format {
        "tiff" => {
            let mut fields = metadata.normalized(w, h);
            for (tag, value) in [
                (exif::Tag::BitsPerSample, exif::Value::Short(vec![16; 3])),
                (exif::Tag::Compression, exif::Value::Short(vec![1])),
                (
                    exif::Tag::PhotometricInterpretation,
                    exif::Value::Short(vec![2]),
                ),
                (exif::Tag::SamplesPerPixel, exif::Value::Short(vec![3])),
                (exif::Tag::RowsPerStrip, exif::Value::Long(vec![h])),
                (exif::Tag::PlanarConfiguration, exif::Value::Short(vec![1])),
                (exif::Tag(exif::Context::Tiff, 700), exif::Value::Byte(xmp)),
            ] {
                fields.push(exif::Field {
                    tag,
                    ifd_num: exif::In::PRIMARY,
                    value,
                });
            }
            let samples: Vec<u8> = image
                .as_raw()
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            metadata::write_fields(&fields, Some(&samples))
        }
        "png" => {
            let samples16: Vec<u8> = image
                .as_raw()
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();
            let mut encoder = image::codecs::png::PngEncoder::new(&mut data);
            encoder
                .set_exif_metadata(exif)
                .map_err(|e| fail(e.to_string()))?;
            encoder
                .write_image(&samples16, w, h, image::ExtendedColorType::Rgb16)
                .map_err(|e| fail(e.to_string()))?;
            metadata::png_xmp(&data, &xmp)
        }
        "jpeg" => {
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, quality);
            encoder
                .set_exif_metadata(exif)
                .map_err(|e| fail(e.to_string()))?;
            encoder
                .write_image(&rgb(), w, h, image::ExtendedColorType::Rgb8)
                .map_err(|e| fail(e.to_string()))?;
            metadata::jpeg_metadata(&data, &xmp, metadata)
        }
        "webp" => {
            let encoded = webp::Encoder::from_rgb(&rgb(), w, h).encode(f32::from(quality));
            metadata::webp_metadata(&encoded, &exif, &xmp, w, h)
        }
        "avif" => {
            data =
                crate::thumbnail::render::encode_avif_with_metadata(&rgb(), w, h, quality, exif)?;
            metadata::avif_xmp(&data, &xmp)
        }
        _ => Err(fail("不支持此导出格式")),
    }
}
pub fn extension(format: &str) -> Result<&'static str> {
    match format {
        "jpeg" => Ok("jpg"),
        "tiff" => Ok("tiff"),
        "png" => Ok("png"),
        "webp" => Ok("webp"),
        "avif" => Ok("avif"),
        _ => Err(fail("不支持此导出格式")),
    }
}
pub fn relative_name(
    preset: &Preset,
    source: &Path,
    naming: &Naming,
    sequence: u64,
) -> Result<String> {
    let parsed =
        crate::import::template::parse(&preset.template).map_err(|e| fail(e.to_string()))?;
    let seqs: Vec<_> = parsed
        .seq_widths()
        .into_iter()
        .map(|w| (w, sequence))
        .collect();
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| fail("源文件名称不是合法 Unicode"))?;
    let text = parsed
        .render(&crate::import::template::RenderCtx {
            taken_at: naming.taken_at,
            stem,
            brand: naming.brand.as_deref(),
            model: naming.model.as_deref(),
            seqs: crate::import::template::SeqValues::new(&seqs),
        })
        .text;
    crate::import::template::check_output(&text).map_err(|e| fail(e.to_string()))?;
    for part in text.split('/') {
        let base = part.split('.').next().unwrap_or("").to_uppercase();
        if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && matches!(base.as_bytes()[3], b'1'..=b'9'))
            || part.encode_utf16().count() > 240
        {
            return Err(fail("输出名称包含设备名或过长分段"));
        }
    }
    Ok(format!("{text}.{}", extension(&preset.format)?))
}
fn target_parent(root: &Path, relative: &str) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let rel = Path::new(relative);
    if rel.is_absolute()
        || rel
            .components()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
    {
        return Err(fail("导出目标不能穿越目录"));
    }
    let mut parent = root.clone();
    if let Some(parts) = rel.parent() {
        for part in parts.components() {
            parent.push(part);
            match std::fs::create_dir(&parent) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            let canonical = parent.canonicalize()?;
            if !canonical.starts_with(&root) {
                return Err(fail("导出子目录指向目标目录之外"));
            }
            parent = canonical;
        }
    }
    Ok(parent)
}
#[derive(Debug, PartialEq, Eq)]
pub enum Publication {
    Written(PathBuf),
    Skipped(PathBuf),
}

fn existing_file(target: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(target) {
        Ok(_) if target.is_file() => Ok(true),
        Ok(_) => Err(fail("同名导出目标不是文件")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// External editors retain append-only publication through the same engine.
pub fn publish(root: &Path, relative: &str, bytes: &[u8], source: &Path) -> Result<PathBuf> {
    match publish_with_policy(root, relative, bytes, source, super::ExistingFile::Append)? {
        Publication::Written(path) => Ok(path),
        Publication::Skipped(_) => unreachable!("append always publishes a new file"),
    }
}

/// Publish complete files atomically; no existence check is used to arbitrate writers.
pub fn publish_with_policy(
    root: &Path,
    relative: &str,
    bytes: &[u8],
    source: &Path,
    policy: super::ExistingFile,
) -> Result<Publication> {
    use super::ExistingFile;
    let parent = target_parent(root, relative)?;
    let rel = Path::new(relative);
    let stem = rel
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| fail("输出名称无效"))?;
    let ext = rel
        .extension()
        .and_then(|s| s.to_str())
        .ok_or_else(|| fail("输出扩展名无效"))?;
    let source = source.canonicalize()?;
    for index in 0..=99_999 {
        let target = parent.join(format!(
            "{}.{}",
            crate::import::plan::collision_stem(stem, index),
            ext
        ));
        let exists = existing_file(&target)?;
        if policy == ExistingFile::Skip && exists {
            return Ok(Publication::Skipped(target));
        }
        if target == source || (exists && target.canonicalize()? == source) {
            if policy == ExistingFile::Append {
                continue;
            }
            return Err(fail("不能覆盖源照片，请选择其他导出目录或使用追加"));
        }
        let result = if policy == ExistingFile::Overwrite {
            crate::fs_atomic::write(&target, bytes)
        } else {
            crate::fs_atomic::write_new(&target, bytes)
        };
        match result {
            Ok(()) => return Ok(Publication::Written(target)),
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if policy == ExistingFile::Skip {
                    if existing_file(&target)? {
                        return Ok(Publication::Skipped(target));
                    }
                    return Err(e.into());
                }
                if policy == ExistingFile::Append {
                    continue;
                }
                return Err(e.into());
            }
            Err(e) => return Err(e),
        }
    }
    Err(fail("导出重名数量超过上限"))
}
/// Resolve skip before metadata reads/RAW rendering; final publication also checks races.
pub fn skip_existing(
    source: &Path,
    preset: &Preset,
    naming: &Naming,
    sequence: u64,
) -> Result<Option<PathBuf>> {
    if preset.existing_file != super::ExistingFile::Skip {
        return Ok(None);
    }
    let relative = relative_name(preset, source, naming, sequence)?;
    let parent = target_parent(Path::new(&preset.directory), &relative)?;
    let target = parent.join(
        Path::new(&relative)
            .file_name()
            .ok_or_else(|| fail("输出名称无效"))?,
    );
    Ok(existing_file(&target)?.then_some(target))
}

pub fn execute(
    source: &Path,
    captured: &VariantSnapshot,
    preset: &Preset,
    naming: &Naming,
    metadata: &Metadata,
    sequence: u64,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
) -> Result<Publication> {
    let validation = preset.validate(true);
    if !validation.errors.is_empty() {
        return Err(fail(format!("导出预设无效：{:?}", validation.errors)));
    }
    let relative = relative_name(preset, source, naming, sequence)?;
    if let Some(target) = skip_existing(source, preset, naming, sequence)? {
        return Ok(Publication::Skipped(target));
    }
    let image = super::render_for_preset(source, captured, lens, lut, preset)?;
    let bytes = encode(&image, &preset.format, preset.quality, metadata)?;
    super::check_source(source, captured)?;
    publish_with_policy(
        Path::new(&preset.directory),
        &relative,
        &bytes,
        source,
        preset.existing_file,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collision_policies_survive_cleared_queues_and_preserve_sources() {
        use super::super::ExistingFile::*;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("源.png");
        std::fs::write(&source, b"original").unwrap();
        let target = dir.path().join("子/输出.jpg");
        assert_eq!(
            publish_with_policy(dir.path(), "子/输出.jpg", b"old", &source, Skip).unwrap(),
            Publication::Written(target.canonicalize().unwrap())
        );
        assert_eq!(
            publish_with_policy(dir.path(), "子/输出.jpg", b"new", &source, Skip).unwrap(),
            Publication::Skipped(target.canonicalize().unwrap())
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert_eq!(
            publish_with_policy(dir.path(), "子/输出.jpg", b"new", &source, Overwrite).unwrap(),
            Publication::Written(target.canonicalize().unwrap())
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        for index in 1..=2 {
            assert_eq!(
                publish_with_policy(dir.path(), "子/输出.jpg", b"append", &source, Append).unwrap(),
                Publication::Written(dir.path().join(format!("子/输出_{index:02}.jpg")).canonicalize().unwrap())
            );
        }
        assert!(publish_with_policy(dir.path(), "源.png", b"destroy", &source, Overwrite).is_err());
        assert!(matches!(
            publish_with_policy(dir.path(), "源.png", b"skip", &source, Skip).unwrap(),
            Publication::Skipped(_)
        ));
        assert_eq!(
            publish(dir.path(), "源.png", b"append", &source).unwrap(),
            dir.path().join("源_01.png").canonicalize().unwrap()
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
        std::fs::create_dir(dir.path().join("directory.jpg")).unwrap();
        for policy in [Overwrite, Skip, Append] {
            assert!(
                publish_with_policy(dir.path(), "directory.jpg", b"x", &source, policy).is_err()
            );
            assert!(publish_with_policy(dir.path(), "../bad.jpg", b"x", &source, policy).is_err());
        }
    }

    #[test]
    fn concurrent_collision_policies_publish_complete_files_and_clean_temps() {
        use super::super::ExistingFile::*;
        for policy in [Overwrite, Skip, Append] {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("源.bin");
            std::fs::write(&source, b"original").unwrap();
            let results = std::thread::scope(|s| {
                (0..8u8)
                    .map(|value| {
                        let source = &source;
                        let root = dir.path();
                        s.spawn(move || {
                            publish_with_policy(root, "并发.png", &[value; 4096], source, policy)
                                .unwrap()
                        })
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .collect::<Vec<_>>()
            });
            let written = results
                .iter()
                .filter(|r| matches!(r, Publication::Written(_)))
                .count();
            assert_eq!(written, if policy == Skip { 1 } else { 8 });
            let paths: std::collections::BTreeSet<_> = results
                .iter()
                .map(|r| match r {
                    Publication::Written(p) | Publication::Skipped(p) => p,
                })
                .collect();
            assert_eq!(paths.len(), if policy == Append { 8 } else { 1 });
            for path in &paths {
                let bytes = std::fs::read(path).unwrap();
                assert_eq!(bytes.len(), 4096);
                assert!(bytes.iter().all(|v| *v == bytes[0]));
            }
            assert_eq!(
                std::fs::read_dir(dir.path()).unwrap().count(),
                paths.len() + 1
            );
            assert_eq!(std::fs::read(&source).unwrap(), b"original");
        }
    }

    #[cfg(unix)]
    #[test]
    fn overwrite_never_follows_source_symlink_and_replacing_hardlink_keeps_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("源.jpg");
        std::fs::write(&source, b"original").unwrap();
        std::os::unix::fs::symlink(&source, dir.path().join("alias.jpg")).unwrap();
        assert!(
            publish_with_policy(
                dir.path(),
                "alias.jpg",
                b"x",
                &source,
                super::super::ExistingFile::Overwrite
            )
            .is_err()
        );
        std::fs::hard_link(&source, dir.path().join("hard.jpg")).unwrap();
        publish_with_policy(
            dir.path(),
            "hard.jpg",
            b"new",
            &source,
            super::super::ExistingFile::Overwrite,
        )
        .unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
        assert_eq!(std::fs::read(dir.path().join("hard.jpg")).unwrap(), b"new");
    }

    #[test]
    fn skip_existing_file_avoids_decoding_and_keeps_metadata_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原片.jpg");
        std::fs::write(&source, b"not a decodable image").unwrap();
        let stack = crate::store::develop::DevelopStack {
            source_base: crate::store::develop::EditBase::Sooc,
            ..Default::default()
        };
        let captured = VariantSnapshot {
            reference: super::super::VariantRef {
                asset_id: 1,
                variant: "sooc".into(),
            },
            name: "SOOC".into(),
            rel_path: "原片.jpg".into(),
            profile_hash: crate::store::issues::profile_hash(&stack).unwrap(),
            stack,
            source_signature: crate::media::source::source_signature(&source).unwrap(),
        };
        let mut preset = Preset {
            id: "p".into(),
            name: "预设".into(),
            format: "png".into(),
            quality: 90,
            max_edge: 0,
            size_mode: super::super::SizeMode::Original,
            percent: 100,
            directory: dir.path().to_string_lossy().into_owned(),
            template: ":FILENAME".into(),
            existing_file: super::super::ExistingFile::Skip,
        };
        let target = dir.path().join("原片.png");
        std::fs::write(&target, b"existing image").unwrap();
        assert_eq!(
            execute(
                &source,
                &captured,
                &preset,
                &Naming::default(),
                &Metadata::default(),
                1,
                None,
                None
            )
            .unwrap(),
            Publication::Skipped(target.canonicalize().unwrap())
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"existing image");
        preset.existing_file = super::super::ExistingFile::Overwrite;
        assert!(
            execute(
                &source,
                &captured,
                &preset,
                &Naming::default(),
                &Metadata::default(),
                1,
                None,
                None
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"existing image");
    }

    #[test]
    fn five_formats_metadata_and_precision() {
        let image = crate::display::output::Rgb16Image::from_fn(8, 6, |x, y| {
            image::Rgb([(x * 991 + y * 111) as u16, 12345, 54321])
        });
        let md = Metadata {
            keywords: vec!["旅行 & 山水".into()],
            author: Some("崔总".into()),
            copyright: Some("© 光伴".into()),
            description: Some("<相片>".into()),
            ..Default::default()
        };
        for format in ["jpeg", "tiff", "png", "webp", "avif"] {
            let bytes = encode(&image, format, 85, &md).unwrap();
            let decoded = image::load_from_memory(&bytes).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (8, 6), "{format}");
            if matches!(format, "tiff" | "png") {
                assert_eq!(decoded.to_rgb16(), image)
            }
            let parsed = exif::Reader::new()
                .read_from_container(&mut std::io::Cursor::new(&bytes))
                .unwrap();
            assert_eq!(
                parsed
                    .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                    .unwrap()
                    .value
                    .get_uint(0),
                Some(1)
            );
            assert_eq!(
                parsed
                    .get_field(exif::Tag::PixelXDimension, exif::In::PRIMARY)
                    .unwrap()
                    .value
                    .get_uint(0),
                Some(8)
            );
            assert!(!parsed.fields().any(|f| f.ifd_num == exif::In::THUMBNAIL));
            let packet = metadata::native_xmp(&bytes).unwrap().unwrap();
            assert_eq!(packet, md.xmp(), "native {format}");
            let mut readback = Metadata::default();
            readback.merge_xmp(&packet).unwrap();
            assert_eq!(readback.keywords, md.keywords);
            assert_eq!(readback.author, md.author);
            assert_eq!(readback.copyright, md.copyright);
            assert!(
                bytes
                    .windows("旅行 &amp; 山水".len())
                    .any(|p| p == "旅行 &amp; 山水".as_bytes()),
                "{format}"
            );
            assert!(
                bytes
                    .windows("© 光伴".len())
                    .any(|p| p == "© 光伴".as_bytes()),
                "{format}"
            );
        }
    }
    #[test]
    fn illegal_and_concurrent_publication() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("源.png");
        std::fs::write(&source, b"original").unwrap();
        assert!(publish(dir.path(), "../bad.jpg", b"x", &source).is_err());
        assert!(publish(dir.path(), "/bad.jpg", b"x", &source).is_err());
        let results = std::thread::scope(|s| {
            let handles: Vec<_> = (0..4)
                .map(|_| s.spawn(|| publish(dir.path(), "子/输出.jpg", b"image", &source).unwrap()))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<std::collections::BTreeSet<_>>()
        });
        assert_eq!(results.len(), 4);
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
    }
}
