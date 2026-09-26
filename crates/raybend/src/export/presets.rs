//! Versioned preset file interchange; never serializes session state.
//! 暂不启用：保留文件业务能力，产品无入口。设备预设序列化仍正常使用。
use super::Preset;
use crate::{Error, Result};
use serde::Deserialize;
use std::{collections::BTreeSet, io::Read, path::Path};
const LIMIT: usize = 1_048_576;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    presets: Vec<Preset>,
}
pub fn validate(text: &str) -> Result<Vec<Preset>> {
    if text.len() > LIMIT {
        return Err(Error::Unsupported("预设文件超过 1 MiB".into()));
    }
    let mut e: Envelope =
        serde_json::from_str(text).map_err(|e| Error::Unsupported(e.to_string()))?;
    if e.version == 1 {
        for preset in &mut e.presets {
            if preset.format == "tiff" {
                preset.format = "png".into();
            }
            preset.size_mode = if preset.max_edge == 0 {
                super::SizeMode::Original
            } else {
                super::SizeMode::MaxEdge
            };
            preset.percent = 100;
        }
    }
    if e.version < 3 {
        for preset in &mut e.presets {
            preset.existing_file = super::ExistingFile::Append;
        }
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    if ![1, 2, 3].contains(&e.version)
        || e.presets.len() > 256
        || e.presets.iter().any(|p| {
            !p.validate(false).errors.is_empty() || !ids.insert(&p.id) || !names.insert(&p.name)
        })
    {
        return Err(Error::Unsupported(
            "预设文件版本、参数或重复名称无效".into(),
        ));
    }
    Ok(e.presets)
}
pub fn file(path: &Path, content: Option<String>) -> Result<String> {
    if !path.is_absolute() {
        return Err(Error::Unsupported("预设文件需要绝对路径".into()));
    }
    let text = match &content {
        Some(text) => text.clone(),
        None => {
            let file = std::fs::File::open(path)?;
            let mut text = String::new();
            file.take(LIMIT as u64 + 1).read_to_string(&mut text)?;
            text
        }
    };
    validate(&text)?;
    if content.is_some() {
        crate::fs_atomic::write(path, text.as_bytes())?
    }
    Ok(text)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_tiff_migrates_but_current_files_reject_it_and_preserve_percent() {
        let legacy = serde_json::json!({"id":"p","name":"老预设","format":"tiff","quality":90,"maxEdge":2048,"directory":"C:/output","template":":FILENAME"});
        let migrated =
            validate(&serde_json::json!({"version":1,"presets":[legacy.clone()]}).to_string())
                .unwrap();
        assert_eq!(migrated[0].format, "png");
        assert_eq!(migrated[0].size_mode, super::super::SizeMode::MaxEdge);
        assert!(
            validate(&serde_json::json!({"version":2,"presets":[legacy]}).to_string()).is_err()
        );
        let mut p = migrated[0].clone();
        p.size_mode = super::super::SizeMode::Percent;
        p.percent = 33;
        assert_eq!(
            validate(&serde_json::json!({"version":2,"presets":[p.clone()]}).to_string()).unwrap(),
            vec![p]
        );
    }
    #[test]
    fn collision_policy_migrates_append_and_current_version_preserves_all_choices() {
        let legacy = serde_json::json!({"id":"p","name":"预设","format":"png","quality":90,"maxEdge":0,"sizeMode":"original","percent":100,"directory":"C:/out","template":":FILENAME"});
        for version in [1, 2] {
            let presets = validate(
                &serde_json::json!({"version":version,"presets":[legacy.clone()]}).to_string(),
            )
            .unwrap();
            assert_eq!(presets[0].existing_file, super::super::ExistingFile::Append);
        }
        for policy in ["overwrite", "skip", "append"] {
            let mut preset = legacy.clone();
            preset["existingFile"] = policy.into();
            let presets =
                validate(&serde_json::json!({"version":3,"presets":[preset.clone()]}).to_string())
                    .unwrap();
            assert_eq!(
                serde_json::to_value(&presets[0]).unwrap()["existingFile"],
                policy
            );
            preset["existingFile"] = "invalid".into();
            assert!(
                validate(&serde_json::json!({"version":3,"presets":[preset]}).to_string()).is_err()
            );
        }
    }

    #[test]
    fn envelope_rejects_future_versions_invalid_values_and_runtime_fields() {
        let p = Preset {
            id: "p".into(),
            name: "中文".into(),
            format: "png".into(),
            quality: 90,
            max_edge: 0,
            size_mode: crate::export::SizeMode::Original,
            percent: 100,
            directory: "C:/输出".into(),
            template: ":FILENAME".into(),
            existing_file: crate::export::ExistingFile::Append,
        };
        let text = serde_json::json!({"version":1,"presets":[p.clone()]}).to_string();
        assert_eq!(validate(&text).unwrap()[0], p);
        for text in [
            "{}".into(),
            serde_json::json!({"version":4,"presets":[p.clone()]}).to_string(),
            serde_json::json!({"version":1,"presets":[p.clone(),p.clone()]}).to_string(),
            serde_json::json!({"version":1,"presets":[p],"queues":[]}).to_string(),
            "x".repeat(LIMIT + 1),
        ] {
            assert!(validate(&text).is_err())
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("中文.json");
        file(&path, Some(text.clone())).unwrap();
        assert_eq!(file(&path, None).unwrap(), text);
        assert!(file(Path::new("relative.json"), None).is_err());
    }
}
