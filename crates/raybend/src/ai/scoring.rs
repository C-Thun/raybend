//! 每类独立余弦分数。没有阈值校准的矩阵可以 probe，不能生成产品标签。
use crate::{Error, Result};
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Concept {
    pub key: String,
    pub prompts: Vec<String>,
    pub embedding: Vec<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative_embedding: Option<Vec<f32>>,
}
pub fn score(image: &[f32], concepts: &[Concept]) -> Result<Vec<f32>> {
    if image.is_empty() || image.len() > 4096 || concepts.is_empty() || concepts.len() > 32 {
        return Err(Error::Unsupported("AI 特征维度或类别数无效".into()));
    }
    let normalize = |v: &[f32]| -> Result<f64> {
        if v.len() != image.len() || v.iter().any(|x| !x.is_finite()) {
            return Err(Error::Unsupported("AI 特征包含非法值或维度不符".into()));
        }
        let norm = v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
        if !norm.is_finite() || norm < 1e-12 {
            return Err(Error::Unsupported("AI 特征范数无效".into()));
        }
        Ok(norm)
    };
    let image_norm = normalize(image)?;
    let mut keys = std::collections::BTreeSet::new();
    concepts
        .iter()
        .map(|c| {
            if c.key.is_empty() || c.key.len() > 64 || !keys.insert(&c.key) {
                return Err(Error::Unsupported("AI 类别身份无效或重复".into()));
            }
            let norm = normalize(&c.embedding)?;
            let dot = image
                .iter()
                .zip(&c.embedding)
                .map(|(a, b)| f64::from(*a) * f64::from(*b))
                .sum::<f64>();
            let positive = (dot / (image_norm * norm)).clamp(-1., 1.);
            let value = if let Some(negative) = &c.negative_embedding {
                let norm = normalize(negative)?;
                let dot = image
                    .iter()
                    .zip(negative)
                    .map(|(a, b)| f64::from(*a) * f64::from(*b))
                    .sum::<f64>();
                // 半差值仍在[-1,1]；它是相似度判据，不是概率。
                (positive - (dot / (image_norm * norm)).clamp(-1., 1.)) * 0.5
            } else {
                positive
            };
            Ok(value.clamp(-1., 1.) as f32)
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn c(v: Vec<f32>, key: &str) -> Concept {
        Concept {
            key: key.into(),
            prompts: vec![],
            embedding: v,
            negative_embedding: None,
        }
    }
    #[test]
    fn independent_scores_allow_multiple_categories_and_no_softmax() {
        let scores = score(
            &[1., 0.],
            &[
                c(vec![1., 0.], "a"),
                c(vec![1., 0.], "b"),
                c(vec![-1., 0.], "c"),
            ],
        )
        .unwrap();
        assert_eq!(scores, [1., 1., -1.]);
    }
    #[test]
    fn contrastive_classes_reject_confusers_without_changing_plain_scores() {
        let mut contrast = c(vec![1., 0.], "海");
        contrast.negative_embedding = Some(vec![0., 1.]);
        assert_eq!(score(&[1., 0.], &[contrast.clone()]).unwrap(), [0.5]);
        assert_eq!(score(&[0., 1.], &[contrast.clone()]).unwrap(), [-0.5]);
        contrast.negative_embedding = Some(vec![0.; 2]);
        assert!(score(&[1., 0.], &[contrast.clone()]).is_err());
        contrast.negative_embedding = Some(vec![f32::NAN; 2]);
        assert!(score(&[1., 0.], &[contrast]).is_err());
    }
    #[test]
    fn invalid_vectors_and_duplicate_concepts_are_rejected() {
        for image in [
            vec![],
            vec![0., 0.],
            vec![f32::NAN, 0.],
            vec![f32::INFINITY, 0.],
        ] {
            assert!(score(&image, &[c(vec![1., 0.], "a")]).is_err());
        }
        assert!(score(&[1., 0.], &[c(vec![1.], "a")]).is_err());
        assert!(score(&[1., 0.], &[c(vec![1., 0.], "a"), c(vec![1., 0.], "a")]).is_err());
        assert_eq!(
            score(&[f32::MAX, 0.], &[c(vec![f32::MAX, 0.], "a")]).unwrap(),
            [1.]
        );
    }
}
