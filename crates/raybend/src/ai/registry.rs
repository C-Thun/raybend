//! 可信模型的不可变目录与版本化 ready 指针；校验/加载失败不覆盖上次可用状态。
use super::pack::{self, Verified};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
/// 崔总认可的 TinyCLIP v1 折中阈值包；可信身份独立于包内自报状态。
pub const APPROVED_MANIFESTS: &[&str] =
    &["775a029ab0956f88acbd40dcb10e5fbbc174e3bb624c656b23923979684bbfc7"];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    version: u32,
    manifest_sha256: String,
}
fn approved(hash: &str, trusted: &[&str]) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        || !trusted.contains(&hash)
    {
        return Err(Error::Unsupported(
            "标签模型尚未通过验证或不属于本版本的可信包".into(),
        ));
    }
    Ok(())
}
pub fn selected(models: &Path, trusted: &[&str]) -> Result<Option<(String, Verified)>> {
    let path = models.join("ready-v1.json");
    if !path.exists() {
        return Ok(None);
    }
    let p: Pointer =
        serde_json::from_slice(&crate::fs_asset::read_limited(&path, 1024, "AI ready")?)?;
    if p.version != 1 {
        return Err(Error::Unsupported("AI ready 版本不受支持".into()));
    }
    approved(&p.manifest_sha256, trusted)?;
    let pack = pack::verify(&models.join(&p.manifest_sha256), &p.manifest_sha256, true)?;
    Ok(Some((p.manifest_sha256, pack)))
}
pub fn install(
    models: &Path,
    source: &Path,
    trusted: &[&str],
    probe: impl FnOnce(&Path) -> Result<()>,
) -> Result<PathBuf> {
    let raw =
        crate::fs_asset::read_limited(&source.join("manifest.json"), 64 * 1024, "AI manifest")?;
    let hash = crate::fs_asset::hash_bytes(&raw);
    approved(&hash, trusted)?;
    let directory = pack::install(source, models, &hash)?;
    // 元数据不替代实际图加载；子进程合同探针成功后才切换 ready。
    probe(&directory)?;
    crate::fs_atomic::write(
        &models.join("ready-v1.json"),
        &serde_json::to_vec(&Pointer {
            version: 1,
            manifest_sha256: hash,
        })?,
    )?;
    Ok(directory)
}
pub fn uninstall(models: &Path, trusted: &[&str], in_use: bool) -> Result<bool> {
    if in_use {
        return Err(Error::Unsupported(
            "模型正在识别，请先暂停任务并等待当前照片完成".into(),
        ));
    }
    let pointer = models.join("ready-v1.json");
    if !pointer.exists() {
        return Ok(false);
    }
    let pointer: Pointer =
        serde_json::from_slice(&crate::fs_asset::read_limited(&pointer, 1024, "AI ready")?)?;
    if pointer.version != 1 {
        return Err(Error::Unsupported("AI ready 版本不受支持".into()));
    }
    approved(&pointer.manifest_sha256, trusted)?;
    let hash = pointer.manifest_sha256;
    std::fs::remove_file(models.join("ready-v1.json"))?;
    // 移除指针后即不再对外 ready；删除失败也不能重新冒认可用。
    let directory = models.join(hash);
    if directory.exists() {
        std::fs::remove_dir_all(directory)?;
    }
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shipped_metadata_matches_trust_list_and_all_seven_calibrated_concepts() {
        let raw = include_bytes!("../../assets/ai/tinyclip-v1/manifest.json");
        let hash = crate::fs_asset::hash_bytes(raw);
        assert_eq!(APPROVED_MANIFESTS, [&*hash]);
        let manifest: pack::Manifest = serde_json::from_slice(raw).unwrap();
        assert_eq!(manifest.format, 2);
        assert!(manifest.thresholds_calibrated);
        assert_eq!(
            manifest.revision,
            "95ec8197b3f2fe7f747865c61ca556cf0768b2f7"
        );
        let data = include_bytes!("../../assets/ai/tinyclip-v1/classes.json");
        let file = manifest
            .files
            .iter()
            .find(|f| f.name == "classes.json")
            .unwrap();
        assert_eq!(file.bytes, data.len() as u64);
        assert_eq!(file.sha256, crate::fs_asset::hash_bytes(data));
        let classes: Vec<pack::Class> = serde_json::from_slice(data).unwrap();
        assert_eq!(classes.len(), 7);
        assert_eq!(
            classes.iter().map(|c| c.zh.as_str()).collect::<Vec<_>>(),
            ["人", "鸟", "车", "山", "海", "森林", "夜景"]
        );
        for class in &classes {
            assert!(class.threshold.is_some());
            assert_eq!(class.concept.embedding.len(), 512);
        }
        assert!(classes[0].concept.negative_embedding.is_some());
        assert!(classes[4].concept.negative_embedding.is_some());
        super::super::scoring::score(
            &vec![1.; 512],
            &classes.into_iter().map(|c| c.concept).collect::<Vec<_>>(),
        )
        .unwrap();
    }
    #[test]
    fn failed_graph_probe_keeps_ready_and_uninstall_never_erases_photo_data() {
        let d = tempfile::tempdir().unwrap();
        let source = d.path().join("源");
        let hash = super::super::pack::tests::pack(&source, true);
        let models = d.path().join("模型");
        assert!(install(&models, &source, &[], |_| Ok(())).is_err());
        assert!(
            install(&models, &source, &[&hash], |_| Err(Error::Unsupported(
                "wrong graph".into()
            )))
            .is_err()
        );
        assert!(selected(&models, &[&hash]).unwrap().is_none());
        install(&models, &source, &[&hash], |_| Ok(())).unwrap();
        assert_eq!(selected(&models, &[&hash]).unwrap().unwrap().0, hash);
        assert!(uninstall(&models, &[&hash], true).is_err());
        std::fs::write(models.join(&hash).join("image_encoder.onnx"), b"corrupt").unwrap();
        assert!(selected(&models, &[&hash]).is_err());
        assert!(uninstall(&models, &[&hash], false).unwrap());
        assert!(source.join("image_encoder.onnx").exists());
        assert!(selected(&models, &[&hash]).unwrap().is_none());
    }
}
