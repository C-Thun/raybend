//! 目录导入预处理：三级扫描、伴生封面或固定样片、4:3 封面烘焙。

use super::lut::{Lut, SAMPLE_WEBP};
use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

/// 导入与哈希共用的文件大小上限。
pub const MAX_LUT_BYTES: u64 = 128 * 1024 * 1024;

/// 文件身份按原始字节计算，与文件名、目录、分类无关。
pub fn file_hash(path: &Path) -> Result<String> {
    hash_reader(std::fs::File::open(path)?, MAX_LUT_BYTES)
}

fn hash_reader(reader: impl std::io::Read, limit: u64) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut reader = reader.take(limit + 1);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 32 * 1024];
    let mut length = 0_u64;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        length += count as u64;
        if length > limit {
            return Err(unsupported("LUT 文件超过大小上限"));
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// 发布同一份字节快照，再对应用内文件计算身份；来源中途改名 / 替换不会让记录错配。
pub fn copy_for_import(source: &Path, target: &Path) -> Result<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(source)?
        .take(MAX_LUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_LUT_BYTES {
        return Err(unsupported("LUT 文件超过 128 MB"));
    }
    crate::fs_atomic::write(target, &bytes)?;
    file_hash(target)
}

const COVER_SUFFIXES: [&str; 7] = ["jpg", "jpeg", "png", "tif", "tiff", "avif", "webp"];

fn unsupported(message: impl Into<String>) -> Error {
    Error::Unsupported(message.into())
}

pub fn discover(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.is_dir() {
        return Err(unsupported("LUT 导入来源必须是目录"));
    }
    let mut dirs = vec![(root.to_path_buf(), 0_u8)];
    let mut files = Vec::new();
    while let Some((dir, depth)) = dirs.pop() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() && depth < 2 {
                dirs.push((path, depth + 1));
            } else if kind.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        ["cube", "png", "tif", "tiff"].contains(&ext.to_ascii_lowercase().as_str())
                    })
            {
                files.push(path);
            }
            if files.len() > 1000 || dirs.len() > 1000 {
                return Err(unsupported("导入目录超过 1000 个候选文件/子目录"));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// 同目录的同名 CUBE 存在时，图片是伴生封面，而不是第二个 Hald LUT。
pub fn source_cube_exists(path: &Path) -> bool {
    let Some(stem) = path.file_stem() else {
        return false;
    };
    std::fs::read_dir(path.parent().unwrap_or(Path::new(".")))
        .ok()
        .is_some_and(|entries| {
            entries.flatten().any(|entry| {
                let candidate = entry.path();
                candidate.is_file()
                    && candidate.file_stem().is_some_and(|value| {
                        value.to_string_lossy().to_lowercase()
                            == stem.to_string_lossy().to_lowercase()
                    })
                    && candidate
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("cube"))
            })
        })
}

pub fn companion_image(lut_path: &Path) -> Option<PathBuf> {
    let stem = lut_path.file_stem()?.to_string_lossy().to_lowercase();
    let entries = std::fs::read_dir(lut_path.parent()?).ok()?;
    let candidates: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path != lut_path
                && path
                    .file_stem()
                    .is_some_and(|value| value.to_string_lossy().to_lowercase() == stem)
        })
        .collect();
    COVER_SUFFIXES.iter().find_map(|extension| {
        candidates
            .iter()
            .find(|path| {
                path.extension()
                    .is_some_and(|value| value.to_string_lossy().eq_ignore_ascii_case(extension))
            })
            .cloned()
    })
}

// 仅版本化派生封面缓存，不改变 app.db 的业务 schema。
const COVER_VERSION: u32 = 2;
const COVER_MANIFEST: &str = "cover-meta.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct CoverManifest {
    version: u32,
    /// None = 内置样片；Some = 已复制进 LUT 内部目录的伴生原图。
    companion: Option<String>,
}

fn render_cover(lut: &Lut, companion: Option<&Path>) -> Result<(Vec<u8>, u32, u32)> {
    let image = if let Some(path) = companion {
        image::open(path).map_err(|error| {
            unsupported(format!("LUT 伴生封面无法解码 {}：{error}", path.display()))
        })?
    } else {
        image::load_from_memory_with_format(SAMPLE_WEBP, image::ImageFormat::WebP)
            .map_err(|error| unsupported(format!("内置 LUT 样片无法解码：{error}")))?
    };
    let target = if companion.is_some() && image.width() >= 768 && image.height() >= 576 {
        (768, 576)
    } else {
        (384, 288)
    };
    let mut rgb = image.to_rgb8();
    if companion.is_none() {
        // 同编辑器：在完整 sRGB 图像上应用同一套 LUT，然后缩图。
        lut.apply_rgb8(rgb.as_mut(), 1.0)?;
    }
    let rgb = image::DynamicImage::ImageRgb8(rgb)
        .resize_to_fill(target.0, target.1, image::imageops::FilterType::Lanczos3)
        .to_rgb8();
    let bytes = webp::Encoder::from_rgb(rgb.as_raw(), target.0, target.1)
        .encode_simple(false, 80.0)
        .map_err(|error| unsupported(format!("LUT 封面编码失败：{error:?}")))?;
    Ok((bytes.to_vec(), target.0, target.1))
}

pub fn cover_image(lut_path: &Path, lut: &Lut) -> Result<(Vec<u8>, u32, u32)> {
    render_cover(lut, companion_image(lut_path).as_deref())
}

fn publish_cover(cover_path: &Path, lut: &Lut, companion: Option<&Path>) -> Result<Vec<u8>> {
    let directory = cover_path
        .parent()
        .ok_or_else(|| unsupported("封面路径没有父目录"))?;
    let (bytes, _, _) = render_cover(lut, companion)?;
    let companion = if let Some(source) = companion {
        let extension = source
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| unsupported("伴生封面扩展名无效"))?;
        if !COVER_SUFFIXES.contains(&extension.as_str()) {
            return Err(unsupported("伴生封面格式无效"));
        }
        let filename = format!("cover-source.{extension}");
        let destination = directory.join(&filename);
        if source != destination {
            crate::fs_atomic::write(&destination, &std::fs::read(source)?)?;
        }
        Some(filename)
    } else {
        None
    };
    crate::fs_atomic::write(cover_path, &bytes)?;
    // 最后发布版本；中途失败时下次读取会重试，不能把旧封面误标成新版本。
    let manifest = serde_json::to_vec(&CoverManifest {
        version: COVER_VERSION,
        companion,
    })
    .map_err(|error| unsupported(format!("封面缓存版本写入失败：{error}")))?;
    crate::fs_atomic::write(&directory.join(COVER_MANIFEST), &manifest)?;
    Ok(bytes)
}

/// 导入时连同封面原图一起留存；外部目录移走也能重建伴生封面。
pub fn import_cover(original_lut: &Path, lut: &Lut, cover_path: &Path) -> Result<()> {
    publish_cover(cover_path, lut, companion_image(original_lut).as_deref()).map(|_| ())
}

/// 读取时修复旧编码器生成的封面。缓存可再生，LUT 身份、分类与照片引用不变。
pub fn cached_cover(lut_path: &Path, cover_path: &Path, original_lut: &Path) -> Result<Vec<u8>> {
    let directory = cover_path
        .parent()
        .ok_or_else(|| unsupported("封面路径没有父目录"))?;
    let manifest_path = directory.join(COVER_MANIFEST);
    let manifest = match std::fs::read(&manifest_path) {
        Ok(bytes) => Some(
            serde_json::from_slice::<CoverManifest>(&bytes)
                .map_err(|error| unsupported(format!("封面缓存版本损坏：{error}")))?,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(ref manifest) = manifest {
        if manifest.version > COVER_VERSION {
            return Err(unsupported("封面由更新版本生成，当前版本不能覆盖"));
        }
        if manifest.version == COVER_VERSION && cover_path.is_file() {
            return Ok(std::fs::read(cover_path)?);
        }
    }
    // LUT 文件丢失仍保留可展示的封面。
    if !lut_path.is_file() {
        return Ok(std::fs::read(cover_path)?);
    }
    let companion = if let Some(manifest) = manifest {
        manifest
            .companion
            .map(|filename| {
                let valid = COVER_SUFFIXES
                    .iter()
                    .any(|ext| filename == format!("cover-source.{ext}"));
                if !valid {
                    return Err(unsupported("封面原图路径无效"));
                }
                Ok(directory.join(filename))
            })
            .transpose()?
    } else if original_lut.is_file() {
        companion_image(original_lut)
    } else {
        // 老版本未记录封面来源。源目录也丢失时不能把自带封面误换成样片。
        return Ok(std::fs::read(cover_path)?);
    };
    publish_cover(cover_path, &Lut::load(lut_path)?, companion.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vectors_and_small_size_boundary() {
        assert_eq!(
            hash_reader(&b""[..], 3).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hash_reader(&b"abc"[..], 3).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(hash_reader(&b"abcd"[..], 3).is_err());
        assert!(hash_reader(&b"a"[..], 0).is_err());
        assert!(hash_reader(&b""[..], 0).is_ok());
    }

    #[test]
    fn hashes_use_bytes_and_private_copy_survives_source_changes() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("测试曲线.CUBE");
        let renamed = temp.path().join("另一个名称.cube");
        let target = temp.path().join("private/source.cube");
        std::fs::write(&source, b"abc").unwrap();
        std::fs::copy(&source, &renamed).unwrap();
        let hash = file_hash(&source).unwrap();
        assert_eq!(hash, file_hash(&renamed).unwrap());
        assert_eq!(hash, copy_for_import(&source, &target).unwrap());
        std::fs::write(&source, b"abd").unwrap();
        assert_ne!(hash, file_hash(&source).unwrap());
        assert_eq!(hash, file_hash(&target).unwrap());
        std::fs::remove_file(&target).unwrap();
        assert!(file_hash(&target).is_err());
        assert_eq!(hash, copy_for_import(&renamed, &target).unwrap());
        std::fs::write(&target, b"corrupt").unwrap();
        assert_eq!(hash, copy_for_import(&renamed, &target).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), b"abc");
        assert!(copy_for_import(&temp.path().join("missing"), &target).is_err());
        assert_eq!(file_hash(&target).unwrap(), hash);
    }

    #[test]
    fn companion_cube_detection_handles_case_unicode_and_non_files() {
        let temp = tempfile::tempdir().unwrap();
        let image = temp.path().join("调色.PNG");
        let cube = temp.path().join("调色.CUBE");
        std::fs::write(&image, b"image").unwrap();
        assert!(!source_cube_exists(&image));
        std::fs::write(&cube, b"lut").unwrap();
        assert!(source_cube_exists(&image));
        assert_eq!(companion_image(&cube).unwrap(), image);
        std::fs::remove_file(&cube).unwrap();
        std::fs::create_dir(&cube).unwrap();
        assert!(!source_cube_exists(&image));
        assert!(!source_cube_exists(&temp.path().join("missing/other.png")));
    }

    #[test]
    fn discover_stops_after_two_child_levels_and_skips_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let second = temp.path().join("one/two");
        std::fs::create_dir_all(second.join("three")).unwrap();
        for path in [
            temp.path().join("a.cube"),
            second.join("b.cube"),
            second.join("three/c.cube"),
        ] {
            std::fs::write(path, "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        }
        assert_eq!(discover(temp.path()).unwrap().len(), 2);
    }

    #[test]
    fn companion_cover_keeps_saturated_colors_after_lossy_encoding() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("color.cube");
        std::fs::write(&path, "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        let expected = image::Rgb([230, 110, 20]);
        image::RgbImage::from_pixel(384, 288, expected)
            .save(temp.path().join("color.png"))
            .unwrap();
        let lut = Lut::load(&path).unwrap();
        let (bytes, _, _) = cover_image(&path, &lut).unwrap();
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
            .unwrap()
            .to_rgb8();
        let actual = decoded.get_pixel(192, 144);
        for channel in 0..3 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= 8,
                "channel {channel}: expected {expected:?}, decoded {actual:?}"
            );
        }
    }

    fn identity_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        path
    }

    #[test]
    fn generated_sample_agrees_with_the_editor_pipeline() {
        use crate::develop::pipeline::{DevelopStages, LinearImage, render_develop};
        use crate::develop::{curve::CurveSet, params::DevelopParams};
        // 非恒等的色彩变换，避免两边都没应用 LUT 也通过。
        let lut =
            Lut::parse_cube("LUT_1D_SIZE 3\n0.02 0.03 0.01\n0.65 0.42 0.35\n1 0.85 0.8\n").unwrap();
        let sample = image::load_from_memory_with_format(SAMPLE_WEBP, image::ImageFormat::WebP)
            .unwrap()
            .to_rgb8();
        let source =
            LinearImage::from_srgb8(sample.width(), sample.height(), sample.as_raw()).unwrap();
        let stages = DevelopStages {
            lut: Some(&lut),
            ..Default::default()
        };
        let rendered = render_develop(
            &source,
            &DevelopParams::default(),
            &CurveSet::default(),
            &stages,
        );
        let expected = image::DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(sample.width(), sample.height(), rendered).unwrap(),
        )
        .resize_to_fill(384, 288, image::imageops::FilterType::Lanczos3)
        .to_rgb8();
        let (bytes, w, h) = render_cover(&lut, None).unwrap();
        assert_eq!((w, h), (384, 288));
        let actual = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
            .unwrap()
            .to_rgb8();
        let average_error = expected
            .as_raw()
            .iter()
            .zip(actual.as_raw())
            .map(|(left, right)| f64::from(left.abs_diff(*right)))
            .sum::<f64>()
            / expected.as_raw().len() as f64;
        assert!(
            average_error < 4.0,
            "封面与编辑器像素平均差 {average_error}"
        );
    }

    #[test]
    fn legacy_cover_rebuilds_once_and_missing_lut_keeps_preview() {
        let dir = tempfile::tempdir().unwrap();
        let original = identity_file(dir.path(), "原片.cube");
        let internal = dir.path().join("private");
        std::fs::create_dir(&internal).unwrap();
        let lut_path = internal.join("source.cube");
        std::fs::copy(&original, &lut_path).unwrap();
        let cover = internal.join("cover.webp");
        std::fs::write(&cover, b"old-desaturated-cover").unwrap();
        let bytes = cached_cover(&lut_path, &cover, &original).unwrap();
        assert_ne!(bytes, b"old-desaturated-cover");
        assert!(internal.join(COVER_MANIFEST).is_file());
        // 不存在外部文件仍直接读新版本，且 LUT 丢失不会导致封面也丢失。
        std::fs::remove_file(original).unwrap();
        std::fs::remove_file(lut_path.clone()).unwrap();
        assert_eq!(
            cached_cover(&lut_path, &cover, Path::new("missing.cube")).unwrap(),
            bytes
        );
    }

    #[test]
    fn imported_companion_survives_original_directory_removal_and_cache_loss() {
        let dir = tempfile::tempdir().unwrap();
        let original_dir = dir.path().join("external");
        std::fs::create_dir(&original_dir).unwrap();
        let original = identity_file(&original_dir, "film.cube");
        // LUT 会反转颜色，伴生图必须保持自身颜色、不能再应用 LUT。
        std::fs::write(&original, "LUT_1D_SIZE 2\n1 1 1\n0 0 0\n").unwrap();
        image::RgbImage::from_pixel(384, 288, image::Rgb([230, 110, 20]))
            .save(original_dir.join("film.PNG"))
            .unwrap();
        let internal = dir.path().join("private");
        std::fs::create_dir(&internal).unwrap();
        let lut_path = internal.join("source.cube");
        std::fs::copy(&original, &lut_path).unwrap();
        let cover = internal.join("cover.webp");
        import_cover(&original, &Lut::load(&lut_path).unwrap(), &cover).unwrap();
        let before = std::fs::read(&cover).unwrap();
        std::fs::remove_dir_all(original_dir).unwrap();
        std::fs::remove_file(&cover).unwrap();
        let after = cached_cover(&lut_path, &cover, &original).unwrap();
        assert_eq!(before, after);
        let decoded = image::load_from_memory(&after).unwrap().to_rgb8();
        assert!(decoded.get_pixel(192, 144)[0] > 220);
    }

    #[test]
    fn legacy_unknown_source_is_preserved_and_future_version_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let lut_path = identity_file(dir.path(), "source.cube");
        let cover = dir.path().join("cover.webp");
        std::fs::write(&cover, b"custom-cover").unwrap();
        assert_eq!(
            cached_cover(&lut_path, &cover, Path::new("missing.cube")).unwrap(),
            b"custom-cover"
        );
        let manifest = dir.path().join(COVER_MANIFEST);
        std::fs::write(&manifest, br#"{"version":999,"companion":null}"#).unwrap();
        assert!(cached_cover(&lut_path, &cover, &lut_path).is_err());
        assert_eq!(std::fs::read(&cover).unwrap(), b"custom-cover");
        // 即使旧缓存缺失，非法 companion 也不能越出 LUT 私有目录。
        std::fs::remove_file(&cover).unwrap();
        std::fs::write(&manifest, br#"{"version":1,"companion":"../source.cube"}"#).unwrap();
        assert!(cached_cover(&lut_path, &cover, &lut_path).is_err());
        std::fs::write(&manifest, b"broken json").unwrap();
        assert!(cached_cover(&lut_path, &cover, &lut_path).is_err());
    }

    #[test]
    #[ignore = "外部 CUBE 样本，仅排障手动运行"]
    fn supplied_cubes_cover_probe() {
        let input = std::env::var_os("RAYBEND_LUT_PROBE_DIR").expect("RAYBEND_LUT_PROBE_DIR");
        let output =
            std::env::var_os("RAYBEND_LUT_PROBE_OUTPUT").expect("RAYBEND_LUT_PROBE_OUTPUT");
        std::fs::create_dir_all(&output).unwrap();
        for path in discover(Path::new(&input))
            .unwrap()
            .into_iter()
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("cube"))
            })
        {
            let lut = Lut::load(&path).unwrap();
            let (bytes, w, h) = cover_image(&path, &lut).unwrap();
            assert_eq!((w, h), (384, 288));
            let filename = path.file_stem().unwrap().to_string_lossy();
            std::fs::write(Path::new(&output).join(format!("{filename}.webp")), &bytes).unwrap();
            let actual = image::load_from_memory(&bytes).unwrap().to_rgb8();
            let mut sample = image::load_from_memory(SAMPLE_WEBP).unwrap().to_rgb8();
            lut.apply_rgb8(sample.as_mut(), 1.0).unwrap();
            let expected = image::DynamicImage::ImageRgb8(sample)
                .resize_to_fill(w, h, image::imageops::FilterType::Lanczos3)
                .to_rgb8();
            let chroma = |rgb: &image::RgbImage| -> f64 {
                rgb.pixels()
                    .map(|p| f64::from(p.0.iter().max().unwrap() - p.0.iter().min().unwrap()))
                    .sum::<f64>()
                    / f64::from(w * h)
            };
            let error = expected
                .as_raw()
                .iter()
                .zip(actual.as_raw())
                .map(|(a, b)| f64::from(a.abs_diff(*b)))
                .sum::<f64>()
                / expected.as_raw().len() as f64;
            println!(
                "{filename}: RGB MAE={error:.3}, chroma expected={:.3} decoded={:.3}",
                chroma(&expected),
                chroma(&actual)
            );
            assert!(error < 4.0);
            assert!((chroma(&expected) - chroma(&actual)).abs() < 2.0);
        }
    }

    #[test]
    fn same_stem_cover_wins_and_is_four_by_three() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("film.cube");
        std::fs::write(&path, "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        image::RgbImage::new(800, 600)
            .save(temp.path().join("film.png"))
            .unwrap();
        let lut = Lut::load(&path).unwrap();
        let (bytes, width, height) = cover_image(&path, &lut).unwrap();
        assert_eq!((width, height), (768, 576));
        assert_eq!(&bytes[..4], b"RIFF");
        assert!(
            bytes.windows(4).any(|part| part == b"VP8 "),
            "封面须使用有损 WebP"
        );
        let decoded =
            image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (width, height));
    }
}
