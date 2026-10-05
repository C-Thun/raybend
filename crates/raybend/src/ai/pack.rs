//! 只安装白名单数据。外部可信 manifest 摘要由应用发行配置提供，不能信任包自己盖的 hash。
use super::{preprocess::ResizePolicy, scoring::Concept};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
pub const MAX_PACK_BYTES: u64 = 256 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 64 * 1024;
const FILES: [&str; 3] = ["image_encoder.onnx", "classes.json", "LICENSE.txt"];
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub model_id: String,
    pub revision: String,
    pub ort_version: String,
    pub opset: u32,
    pub image_input: String,
    pub image_output: String,
    pub resize: ResizePolicy,
    pub thresholds_calibrated: bool,
    pub files: Vec<File>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Class {
    pub concept: Concept,
    pub zh: String,
    pub en: String,
    pub threshold: Option<f32>,
}
#[derive(Debug, Clone)]
pub struct Verified {
    pub directory: PathBuf,
    pub manifest: Manifest,
    pub classes: Vec<Class>,
}
fn invalid(reason: &str) -> Error {
    Error::Unsupported(format!("AI 模型包：{reason}"))
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn regular(path: &Path) -> Result<()> {
    if !std::fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(invalid("文件必须是普通数据，不能是符号链接或目录"));
    }
    Ok(())
}
/// 生产包必须有逐类校准阈值与应用认可的摘要；该标记不承诺真实图库准确率。
pub fn verify(
    directory: &Path,
    trusted_manifest_sha256: &str,
    production: bool,
) -> Result<Verified> {
    if !digest(trusted_manifest_sha256) {
        return Err(invalid("可信摘要无效"));
    }
    regular(&directory.join("manifest.json"))?;
    let raw = crate::fs_asset::read_limited(
        &directory.join("manifest.json"),
        MANIFEST_LIMIT,
        "AI manifest",
    )?;
    if crate::fs_asset::hash_bytes(&raw) != trusted_manifest_sha256 {
        return Err(invalid("manifest 不在可信清单内"));
    }
    let manifest: Manifest = serde_json::from_slice(&raw)?;
    if manifest.format != 2
        || manifest.ort_version != "1.28.0"
        || manifest.opset != 17
        || manifest.image_input != "pixel_values"
        || manifest.image_output != "image_features"
        || manifest.model_id.is_empty()
        || manifest.model_id.len() > 256
        || manifest.revision.len() != 40
        || !manifest
            .revision
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        || manifest.files.len() != 3
        || production && !manifest.thresholds_calibrated
    {
        return Err(invalid("格式、运行库或质量状态不受支持"));
    }
    let mut total = 0u64;
    let mut names = std::collections::BTreeSet::new();
    for file in &manifest.files {
        if !FILES.contains(&file.name.as_str())
            || !names.insert(&file.name)
            || file.bytes == 0
            || !digest(&file.sha256)
        {
            return Err(invalid("包内文件声明无效"));
        }
        total = total
            .checked_add(file.bytes)
            .ok_or_else(|| invalid("包大小溢出"))?;
        if total > MAX_PACK_BYTES {
            return Err(invalid("模型包超过 256 MiB"));
        }
        let path = directory.join(&file.name);
        regular(&path)?;
        if std::fs::metadata(&path)?.len() != file.bytes
            || crate::fs_asset::hash_file(&path, file.bytes, "AI 模型")? != file.sha256
        {
            return Err(invalid("模型文件大小或摘要不符"));
        }
    }
    // 额外文件也不安装，避免包里混入可执行内容或无限数量的垃圾文件。
    for entry in std::fs::read_dir(directory)? {
        let name = entry?.file_name();
        if name != "manifest.json" && !FILES.iter().any(|allowed| name == *allowed) {
            return Err(invalid("包内出现未允许的文件"));
        }
    }
    let bytes =
        crate::fs_asset::read_limited(&directory.join("classes.json"), 1024 * 1024, "AI 类别")?;
    let classes: Vec<Class> = serde_json::from_slice(&bytes)?;
    if classes.is_empty() || classes.len() > 32 {
        return Err(invalid("类别数必须在 1–32"));
    }
    for class in &classes {
        if class.concept.embedding.len() != 512
            || class.concept.prompts.len() > 16
            || class.concept.prompts.iter().any(|p| p.len() > 1024)
            || class
                .threshold
                .is_some_and(|v| !v.is_finite() || !(-1.0..=1.0).contains(&v))
            || production && class.threshold.is_none()
        {
            return Err(invalid("类别特征或阈值无效"));
        }
        crate::store::tags::clean_name(&class.zh)?;
        crate::store::tags::clean_name(&class.en)?;
    }
    // 共用评分器校验范数/重复 key/非有限值，不另写第二套矩阵规则。
    super::scoring::score(
        &vec![1.; 512],
        &classes
            .iter()
            .map(|c| c.concept.clone())
            .collect::<Vec<_>>(),
    )?;
    Ok(Verified {
        directory: directory.into(),
        manifest,
        classes,
    })
}
/// 先复制到 staging，再同盘 rename 发布不可变目录；ready 指针更新由设置层在成功后执行。
pub fn install(source: &Path, models: &Path, trusted_manifest_sha256: &str) -> Result<PathBuf> {
    verify(source, trusted_manifest_sha256, true)?;
    std::fs::create_dir_all(models)?;
    let target = models.join(trusted_manifest_sha256);
    if target.exists() && verify(&target, trusted_manifest_sha256, true).is_ok() {
        return Ok(target);
    }
    let stage = models.join(format!(
        ".stage-{}",
        crate::store::ids::new_repository_id()?
    ));
    std::fs::create_dir(&stage)?;
    let result = (|| {
        for name in std::iter::once("manifest.json").chain(FILES) {
            crate::fs_asset::copy_snapshot(
                &source.join(name),
                &stage.join(name),
                if name == "manifest.json" {
                    MANIFEST_LIMIT
                } else {
                    MAX_PACK_BYTES
                },
                "AI 模型",
            )?;
        }
        verify(&stage, trusted_manifest_sha256, true)?;
        // 只替换同一可信摘要的损坏副本；其它版本与 ready 指针保持原样。
        let quarantine = models.join(format!(
            ".broken-{}",
            crate::store::ids::new_repository_id()?
        ));
        let repairing = target.exists();
        if repairing {
            std::fs::rename(&target, &quarantine)?;
        }
        let publish = match std::fs::rename(&stage, &target) {
            Ok(()) => Ok(target.clone()),
            Err(_) if target.exists() => {
                verify(&target, trusted_manifest_sha256, true)?;
                Ok(target.clone())
            }
            Err(error) => Err(error.into()),
        };
        if repairing {
            if publish.is_err() && !target.exists() {
                let _ = std::fs::rename(&quarantine, &target);
            } else {
                let _ = std::fs::remove_dir_all(&quarantine);
            }
        }
        publish
    })();
    if stage.exists() {
        let _ = std::fs::remove_dir_all(&stage);
    }
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn pack(dir: &Path, quality: bool) -> String {
        std::fs::create_dir_all(dir).unwrap();
        let class = Class {
            concept: Concept {
                key: "bird".into(),
                prompts: vec!["a bird".into()],
                embedding: vec![1.; 512],
                negative_embedding: None,
            },
            zh: "鸟".into(),
            en: "bird".into(),
            threshold: quality.then_some(0.3),
        };
        std::fs::write(
            dir.join("classes.json"),
            serde_json::to_vec(&vec![class]).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("image_encoder.onnx"),
            b"test data - never executed",
        )
        .unwrap();
        std::fs::write(dir.join("LICENSE.txt"), b"MIT fixture").unwrap();
        let files = FILES
            .iter()
            .map(|name| {
                let bytes = std::fs::read(dir.join(name)).unwrap();
                File {
                    name: (*name).into(),
                    bytes: bytes.len() as u64,
                    sha256: crate::fs_asset::hash_bytes(&bytes),
                }
            })
            .collect();
        let manifest = Manifest {
            format: 2,
            model_id: "test".into(),
            revision: "a".repeat(40),
            ort_version: "1.28.0".into(),
            opset: 17,
            image_input: "pixel_values".into(),
            image_output: "image_features".into(),
            resize: ResizePolicy::Fit,
            thresholds_calibrated: quality,
            files,
        };
        let raw = serde_json::to_vec(&manifest).unwrap();
        std::fs::write(dir.join("manifest.json"), &raw).unwrap();
        crate::fs_asset::hash_bytes(&raw)
    }
    #[test]
    fn trusted_hash_corruption_extra_files_and_uncalibrated_pack_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let hash = pack(d.path(), false);
        assert!(verify(d.path(), &hash, false).is_ok());
        assert!(verify(d.path(), &hash, true).is_err());
        assert!(verify(d.path(), &"b".repeat(64), false).is_err());
        std::fs::write(d.path().join("image_encoder.onnx"), b"damaged").unwrap();
        assert!(verify(d.path(), &hash, false).is_err());
        let hash = pack(d.path(), true);
        std::fs::write(d.path().join("run.exe"), b"extra").unwrap();
        assert!(verify(d.path(), &hash, true).is_err());
    }
    #[test]
    fn staged_install_is_atomic_idempotent_and_preserves_source() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("来源");
        let hash = pack(&src, true);
        let models = d.path().join("模型");
        let first = install(&src, &models, &hash).unwrap();
        assert_eq!(first, install(&src, &models, &hash).unwrap());
        assert!(src.join("LICENSE.txt").exists());
        assert_eq!(std::fs::read_dir(&models).unwrap().count(), 1);
        std::fs::write(first.join("image_encoder.onnx"), b"broken").unwrap();
        assert!(verify(&first, &hash, true).is_err());
        assert_eq!(install(&src, &models, &hash).unwrap(), first);
        verify(&first, &hash, true).unwrap();
        assert_eq!(std::fs::read_dir(&models).unwrap().count(), 1);
    }
    #[test]
    fn traversal_overflow_and_symbolic_data_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let _ = pack(d.path(), true);
        let path = d.path().join("manifest.json");
        let mut manifest: Manifest =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest.files[0].name = "../outside".into();
        manifest.files[0].bytes = u64::MAX;
        let raw = serde_json::to_vec(&manifest).unwrap();
        std::fs::write(&path, &raw).unwrap();
        assert!(verify(d.path(), &crate::fs_asset::hash_bytes(&raw), true).is_err());
        #[cfg(unix)]
        {
            let hash = pack(d.path(), true);
            std::fs::remove_file(d.path().join("LICENSE.txt")).unwrap();
            std::os::unix::fs::symlink("classes.json", d.path().join("LICENSE.txt")).unwrap();
            assert!(verify(d.path(), &hash, true).is_err());
        }
    }
}
