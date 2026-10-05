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
    // 旧 issue 始终保留现有 sRGB 语义；新工作图单独从 encode_working 进入。
    let srgb = crate::color::icc::srgb_icc().map_err(|error| fail(error.to_string()))?;
    encode_rgb16_with_profile(image, format, quality, metadata, &srgb, true)
}

/// 新处理版本的输出边界：只在成片时从线性 Rec.2020 转换、量化一次，
/// 随后复用原有格式编码器并嵌入同一个目标 ICC。
pub fn encode_working(
    image: &crate::color::working::WorkingImage,
    format: &str,
    quality: u8,
    metadata: &Metadata,
    target: &crate::color::icc::RgbIcc,
) -> Result<Vec<u8>> {
    if !target.accepts_role(crate::color::icc::IccRole::RgbOutput) {
        return Err(fail("目标 ICC 不可用作 RGB 输出"));
    }
    let (w, h) = image.dimensions();
    let srgb = crate::color::icc::srgb_icc().map_err(|error| fail(error.to_string()))?;
    let is_srgb = target.id() == srgb.id();
    validate_encoder_request(w, h, format, quality, is_srgb)?;
    let raw = image
        .to_rgb16_flat(target)
        .map_err(|error| fail(error.to_string()))?;
    let encoded = crate::display::output::Rgb16Image::from_raw(w, h, raw)
        .ok_or_else(|| fail("输出像素尺寸不合法"))?;
    encode_rgb16_with_profile(&encoded, format, quality, metadata, target, is_srgb)
}

fn validate_encoder_request(w: u32, h: u32, format: &str, quality: u8, is_srgb: bool) -> Result<()> {
    if w == 0 || h == 0 {
        return Err(fail("输出尺寸不能为空"));
    }
    if !matches!(format, "tiff" | "png") && !(1..=100).contains(&quality) {
        return Err(fail("质量应为 1–100"));
    }
    if format == "avif" && !is_srgb {
        return Err(fail("当前 AVIF 编码器不能安全标记非 sRGB 输出配置"));
    }
    if !matches!(format, "tiff" | "png" | "jpeg" | "webp" | "avif") {
        return Err(fail("不支持此导出格式"));
    }
    Ok(())
}

fn encode_rgb16_with_profile(
    image: &crate::display::output::Rgb16Image,
    format: &str,
    quality: u8,
    metadata: &Metadata,
    target: &crate::color::icc::RgbIcc,
    is_srgb: bool,
) -> Result<Vec<u8>> {
    let (w, h) = image.dimensions();
    validate_encoder_request(w, h, format, quality, is_srgb)?;
    let exif = metadata.exif_for_profile(w, h, is_srgb)?;
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
            let mut fields = metadata.normalized_for_profile(w, h, is_srgb);
            fields.retain(|field| field.tag != exif::Tag(exif::Context::Tiff, 34675));
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
            fields.push(exif::Field {
                tag: exif::Tag(exif::Context::Tiff, 34675),
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Byte(target.bytes().to_vec()),
            });
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
                .set_icc_profile(target.bytes().to_vec())
                .map_err(|e| fail(e.to_string()))?;
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
                .set_icc_profile(target.bytes().to_vec())
                .map_err(|e| fail(e.to_string()))?;
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
            metadata::webp_metadata(&encoded, &exif, &xmp, target.bytes(), w, h)
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
    suffix: &str,
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
    // 尾号（I00–I99 / ISO / IRA / ILA）直接接在模板主名后，无其它分隔符（I 即分隔）；
    // 设备名/长度检查对含尾号的最终分段生效（specs/export-issue-ordinal.md §2）。
    let text = format!("{text}{suffix}");
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
    suffix: &str,
) -> Result<Option<PathBuf>> {
    if preset.existing_file != super::ExistingFile::Skip {
        return Ok(None);
    }
    let relative = relative_name(preset, source, naming, sequence, suffix)?;
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
    suffix: &str,
) -> Result<Publication> {
    execute_checked(
        source,
        captured,
        preset,
        naming,
        metadata,
        sequence,
        lens,
        lut,
        || Ok(()),
        suffix,
    )
}

pub fn execute_checked(
    source: &Path,
    captured: &VariantSnapshot,
    preset: &Preset,
    naming: &Naming,
    metadata: &Metadata,
    sequence: u64,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
    check: impl Fn() -> Result<()>,
    suffix: &str,
) -> Result<Publication> {
    execute_checked_with_profiles(source, captured, preset, naming, metadata, sequence, lens, lut, check, suffix,
        |_, _| Err(fail("精确导出引用的自定义 ICC 无法解析")))
}

pub fn execute_checked_with_profiles(
    source: &Path, captured: &VariantSnapshot, preset: &Preset, naming: &Naming, metadata: &Metadata,
    sequence: u64, lens: Option<&crate::develop::lens::LensCorrection>, lut: Option<&crate::develop::lut::Lut>,
    check: impl Fn() -> Result<()>, suffix: &str,
    mut profile: impl FnMut(&crate::color::ProfileId, crate::color::icc::IccRole) -> Result<crate::color::icc::RgbIcc>,
) -> Result<Publication> {
    check()?;
    let validation = preset.validate(true);
    if !validation.errors.is_empty() {
        return Err(fail(format!("导出预设无效：{:?}", validation.errors)));
    }
    let relative = relative_name(preset, source, naming, sequence, suffix)?;
    if let Some(target) = skip_existing(source, preset, naming, sequence, suffix)? {
        return Ok(Publication::Skipped(target));
    }
    let target = resolve_output(&preset.output_color, &mut profile)?;
    let bytes = if captured.stack.color.is_some() {
        let image = super::render_working_for_preset(source, captured, lens, lut, preset, &mut profile)?;
        encode_working(&image, &preset.format, preset.quality, metadata, &target)?
    } else {
        let image = super::render_for_preset(source, captured, lens, lut, preset)?;
        let srgb = crate::color::icc::srgb_icc().map_err(|e| fail(e.to_string()))?;
        if srgb.id() == target.id() { encode(&image, &preset.format, preset.quality, metadata)? }
        else {
            let converted = crate::color::icc::input_rgb16_to_working(&srgb, image.as_raw().as_chunks::<3>().0).map_err(|e| fail(e.to_string()))?;
            let working = crate::color::working::WorkingImage::new(image.width(), image.height(), converted).map_err(|e| fail(e.to_string()))?;
            encode_working(&working, &preset.format, preset.quality, metadata, &target)?
        }
    };
    super::check_source(source, captured)?;
    check()?;
    publish_with_policy(
        Path::new(&preset.directory),
        &relative,
        &bytes,
        source,
        preset.existing_file,
    )
}

pub fn resolve_output(
    output: &crate::color::OutputColor,
    mut profile: impl FnMut(&crate::color::ProfileId, crate::color::icc::IccRole) -> Result<crate::color::icc::RgbIcc>,
) -> Result<crate::color::icc::RgbIcc> {
    use crate::color::{OutputColor, icc};
    let target = match output {
        OutputColor::Srgb => icc::srgb_icc().map_err(|e| fail(e.to_string()))?,
        OutputColor::DisplayP3 => icc::display_p3_icc().map_err(|e| fail(e.to_string()))?,
        OutputColor::AdobeRgb => icc::adobe_rgb_icc().map_err(|e| fail(e.to_string()))?,
        OutputColor::CustomRgbIcc { profile_id } => {
            let target = profile(profile_id, icc::IccRole::RgbOutput)?;
            if target.id() != profile_id { return Err(fail("输出 ICC 与固化身份不符")); }
            target
        }
    };
    if !target.accepts_role(icc::IccRole::RgbOutput) { return Err(fail("目标 ICC 不可用作 RGB 输出")); }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_v2_export_resolves_source_once_and_embeds_the_selected_target() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("广色域原片.png");
        let input = crate::color::icc::display_p3_icc().unwrap();
        let output = crate::color::icc::adobe_rgb_icc().unwrap();
        let rgb = crate::display::output::Rgb16Image::from_raw(2, 1, vec![65535, 0, 0, 127, 32768, 60000]).unwrap();
        rgb.save(&source).unwrap();
        let stack = crate::store::develop::DevelopStack {
            source_base: crate::store::develop::EditBase::Sooc,
            color: Some(crate::color::PhotoColorState::new_pipeline(crate::color::SourceColor::AssignedRgbIcc { profile_id: input.id().clone() })),
            ..Default::default()
        };
        let snapshot = VariantSnapshot { reference: super::super::VariantRef { asset_id: 1, variant: "latest".into() }, name: "高精度".into(),
            rel_path: "广色域原片.png".into(), profile_hash: crate::store::issues::profile_hash(&stack).unwrap(), stack,
            source_signature: crate::media::source::source_signature(&source).unwrap() };
        let preset = Preset { output_color: crate::color::OutputColor::AdobeRgb, id: "p".into(), name: "成片".into(), format: "png".into(), quality: 90,
            max_edge: 0, size_mode: super::super::SizeMode::Original, percent: 100, directory: dir.path().join("输出").to_string_lossy().into_owned(),
            template: ":FILENAME".into(), existing_file: super::super::ExistingFile::Append };
        let reads = std::cell::Cell::new(0);
        std::fs::create_dir(dir.path().join("输出")).unwrap();
        let publication = execute_checked_with_profiles(&source, &snapshot, &preset, &Naming::default(), &Metadata::default(), 1,
            None, None, || Ok(()), "", |id, role| {
                reads.set(reads.get() + 1); assert_eq!(id, input.id()); assert_eq!(role, crate::color::icc::IccRole::PhotoInput); Ok(input.clone())
            }).unwrap();
        assert_eq!(reads.get(), 1);
        let Publication::Written(path) = publication else { panic!("export skipped") };
        let decoded = crate::color::input::decode_bitmap(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(decoded.embedded_icc.as_ref().unwrap().id(), output.id());
        let expected_working = crate::color::icc::input_rgb16_to_working_reference(&input, rgb.as_raw().as_chunks::<3>().0).unwrap();
        let expected = crate::color::icc::working_to_output_rgb16_reference(&output, &expected_working).unwrap();
        let actual = decoded.image.to_rgb16();
        for (a, e) in actual.as_raw().iter().zip(expected.iter().flatten()) { assert!(a.abs_diff(*e) <= 32, "{a} vs {e}"); }
        let before = std::fs::read_dir(dir.path().join("输出")).unwrap().count();
        assert!(execute_checked_with_profiles(&source, &snapshot, &preset, &Naming::default(), &Metadata::default(), 1,
            None, None, || Ok(()), "", |_, _| Ok(crate::color::icc::srgb_icc().unwrap())).is_err());
        assert_eq!(std::fs::read_dir(dir.path().join("输出")).unwrap().count(), before);
    }
    #[test]
    fn expired_catalog_blocks_export_publication_after_render_and_external_tiff() {
        use crate::store::db::{CatalogDb, OpenOpts};
        use std::cell::Cell;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原片.png");
        crate::display::output::Rgb16Image::from_fn(8, 6, |x, y| {
            image::Rgb([(x * 1000) as u16, (y * 1000) as u16, 32768])
        })
        .save(&source)
        .unwrap();
        let original = std::fs::read(&source).unwrap();
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
            rel_path: "原片.png".into(),
            profile_hash: crate::store::issues::profile_hash(&stack).unwrap(),
            stack,
            source_signature: crate::media::source::source_signature(&source).unwrap(),
        };
        let output = dir.path().join("输出");
        std::fs::create_dir(&output).unwrap();
        let preset = Preset {
            output_color: crate::color::OutputColor::Srgb,
            id: "p".into(),
            name: "预设".into(),
            format: "png".into(),
            quality: 90,
            max_edge: 0,
            size_mode: super::super::SizeMode::Original,
            percent: 100,
            directory: output.to_string_lossy().into_owned(),
            template: ":FILENAME".into(),
            existing_file: super::super::ExistingFile::Append,
        };
        for external in [false, true] {
            let catalog = CatalogDb::create(
                &dir.path()
                    .join(if external { "external" } else { "export" }),
                "库",
                None,
                OpenOpts::new(None, 0),
            )
            .unwrap();
            let checks = Cell::new(0);
            let check = || {
                checks.set(checks.get() + 1);
                if checks.get() == 2 {
                    catalog.invalidate();
                }
                catalog.ensure_current()
            };
            let result = if external {
                super::super::write_tiff16_checked(
                    &source,
                    &output.join("外部.tif"),
                    &captured,
                    None,
                    None,
                    0,
                    check,
                )
                .map(|_| ())
            } else {
                execute_checked(
                    &source,
                    &captured,
                    &preset,
                    &Naming::default(),
                    &Metadata::default(),
                    1,
                    None,
                    None,
                    check,
                    "ISO",
                )
                .map(|_| ())
            };
            assert!(matches!(result, Err(Error::SessionExpired)));
            assert_eq!(
                checks.get(),
                2,
                "必须在真实渲染后、发布文件前重新检查原会话"
            );
            assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);
            assert_eq!(std::fs::read(&source).unwrap(), original);
        }
    }
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
                Publication::Written(
                    dir.path()
                        .join(format!("子/输出_{index:02}.jpg"))
                        .canonicalize()
                        .unwrap()
                )
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
            output_color: crate::color::OutputColor::Srgb,
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
        let target = dir.path().join("原片ISO.png");
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
                None,
                "ISO",
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
                None,
                "ISO",
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
            if matches!(format, "jpeg" | "tiff" | "png" | "webp") {
                if format == "tiff" {
                    let parsed = exif::Reader::new()
                        .read_from_container(&mut std::io::Cursor::new(&bytes))
                        .unwrap();
                    let field = parsed
                        .get_field(exif::Tag(exif::Context::Tiff, 34675), exif::In::PRIMARY)
                        .expect("TIFF ICC tag absent");
                    assert!(
                        matches!(field.value, exif::Value::Byte(_)),
                        "TIFF ICC tag type: {:?}",
                        field.value
                    );
                }
                let managed = crate::color::input::decode_bitmap(&bytes).unwrap();
                assert_eq!(
                    managed.evidence,
                    crate::color::input::SourceEvidence::EmbeddedIcc,
                    "{format}"
                );
                assert_eq!(
                    managed.embedded_icc.unwrap().id(),
                    crate::color::icc::srgb_icc().unwrap().id(),
                    "{format}"
                );
            }
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
    fn working_output_embeds_matching_p3_and_rejects_unmarked_avif() {
        let working = crate::color::working::WorkingImage::new(
            2,
            1,
            vec![[0.8, 0.12, 0.03], [0.1, 0.65, 0.9]],
        )
        .unwrap();
        let target = crate::color::icc::display_p3_icc().unwrap();
        let expected: Vec<u16> = working
            .to_rgb16(&target)
            .unwrap()
            .into_iter()
            .flatten()
            .collect();
        let metadata = Metadata::default();
        for format in ["png", "tiff", "jpeg", "webp"] {
            let bytes = encode_working(&working, format, 85, &metadata, &target).unwrap();
            let decoded = crate::color::input::decode_bitmap(&bytes).unwrap();
            assert_eq!(decoded.embedded_icc.unwrap().id(), target.id(), "{format}");
            let exif = exif::Reader::new()
                .read_from_container(&mut std::io::Cursor::new(&bytes))
                .unwrap();
            assert_eq!(
                exif.get_field(exif::Tag::ColorSpace, exif::In::PRIMARY)
                    .unwrap()
                    .value
                    .get_uint(0),
                Some(0xffff),
                "{format}"
            );
            if matches!(format, "png" | "tiff") {
                assert_eq!(decoded.image.to_rgb16().as_raw(), &expected, "{format}");
            }
        }
        assert!(encode_working(&working, "avif", 85, &metadata, &target).is_err());
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
