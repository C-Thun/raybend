//! 每库固定词表与逐类阈值；只有验证过的包可转成标签结果。
use crate::{
    Error, Result,
    store::{
        organization,
        photo_tags::{AiEvidence, AiOrigin, AiResult},
    },
};
use rusqlite::{Connection, OptionalExtension, params};
fn name(conn: &Connection, class: &super::pack::Class, zh: bool) -> Result<String> {
    let existing:Option<String>=conn.query_row("SELECT t.display_name FROM photo_ai_vocabulary v JOIN tag_terms t ON t.tag_key=v.tag_key WHERE v.concept_key=?1",[&class.concept.key],|r|r.get(0)).optional()?;
    if let Some(name) = existing {
        return Ok(name);
    }
    let text = if zh { &class.zh } else { &class.en };
    let key = organization::ensure_term(conn, text, None)?;
    conn.execute(
        "INSERT INTO photo_ai_vocabulary(concept_key,tag_key) VALUES(?1,?2)",
        params![class.concept.key, key],
    )?;
    Ok(text.clone())
}
pub fn labels(conn: &Connection, pack: &super::pack::Verified, zh: bool) -> Result<Vec<String>> {
    pack.classes
        .iter()
        .map(|class| name(conn, class, zh))
        .collect()
}
/// 调用方的 catalog 单写者事务保证词表与结果原子；可信包来源的校验在安装/载入边界完成。
pub fn scored(
    conn: &Connection,
    pack: &super::pack::Verified,
    features: &[f32],
    source_key: &str,
    pipeline: &str,
    zh: bool,
    now: i64,
) -> Result<AiResult> {
    if !pack.manifest.thresholds_calibrated || pack.classes.iter().any(|c| c.threshold.is_none()) {
        return Err(Error::Unsupported(
            "模型包尚未校准，不能提交正式标签".into(),
        ));
    }
    let scores = super::scoring::score(
        features,
        &pack
            .classes
            .iter()
            .map(|c| c.concept.clone())
            .collect::<Vec<_>>(),
    )?;
    let mut evidence = Vec::new();
    for (class, score) in pack.classes.iter().zip(scores) {
        let label = name(conn, class, zh)?;
        evidence.push(AiEvidence {
            concept_key: class.concept.key.clone(),
            tag_name: label,
            score,
            accepted: score >= class.threshold.unwrap(),
        });
    }
    let model = pack
        .manifest
        .files
        .iter()
        .find(|f| f.name == "image_encoder.onnx")
        .ok_or_else(|| Error::Unsupported("缺模型摘要".into()))?;
    let result = AiResult {
        source_key: source_key.into(),
        model_sha256: model.sha256.clone(),
        pipeline_sha256: pipeline.into(),
        generated_at: now,
        origin: AiOrigin::Local,
        valid: true,
        evidence,
    };
    result.validate()?;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pack() -> super::super::pack::Verified {
        use super::super::{
            pack::{Class, File, Manifest, Verified},
            preprocess::ResizePolicy,
            scoring::Concept,
        };
        Verified {
            directory: "/tmp/test".into(),
            manifest: Manifest {
                format: 2,
                model_id: "test".into(),
                revision: "a".repeat(40),
                ort_version: "1.28.0".into(),
                opset: 17,
                image_input: "pixel_values".into(),
                image_output: "image_features".into(),
                resize: ResizePolicy::Fit,
                thresholds_calibrated: true,
                files: vec![File {
                    name: "image_encoder.onnx".into(),
                    bytes: 1,
                    sha256: "a".repeat(64),
                }],
            },
            classes: vec![
                Class {
                    concept: Concept {
                        key: "bird".into(),
                        prompts: vec![],
                        embedding: vec![1., 0.],
                        negative_embedding: None,
                    },
                    zh: "鸟".into(),
                    en: "bird".into(),
                    threshold: Some(0.5),
                },
                Class {
                    concept: Concept {
                        key: "sea".into(),
                        prompts: vec![],
                        embedding: vec![0., 1.],
                        negative_embedding: None,
                    },
                    zh: "海".into(),
                    en: "sea".into(),
                    threshold: Some(0.5),
                },
            ],
        }
    }
    #[test]
    fn first_language_is_stable_and_independent_thresholds_allow_empty_and_multiple() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::store::migration::apply(
            &mut c,
            crate::store::migration::DbKind::Catalog,
            crate::store::migration::Backups::none(),
            1,
        )
        .unwrap();
        let p = pack();
        let a = scored(&c, &p, &[1., 1.], "source", &"b".repeat(64), true, 1).unwrap();
        assert!(a.evidence.iter().all(|e| e.accepted));
        assert_eq!(a.evidence[0].tag_name, "鸟");
        let b = scored(&c, &p, &[-1., -1.], "source", &"b".repeat(64), false, 2).unwrap();
        assert!(b.evidence.iter().all(|e| !e.accepted));
        assert_eq!(b.evidence[0].tag_name, "鸟");
        let mut bad = p;
        bad.manifest.thresholds_calibrated = false;
        assert!(scored(&c, &bad, &[1., 1.], "source", &"b".repeat(64), true, 3).is_err());
    }
}
