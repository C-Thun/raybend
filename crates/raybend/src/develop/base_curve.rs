//! RAW 与同张直出位图的亮度分布拟合、机型档案相似度及保守自动调整。
//! 输入是显示域 RGB8；调用者负责把 RAW 按现有显影管线解到该域。
use super::curve::Curve;

const BINS: usize = 256;
const QUANTILES: [f32; 9] = [0.02, 0.08, 0.18, 0.30, 0.50, 0.70, 0.82, 0.92, 0.98];
/// 加权曲线均方根差；约对应显示亮度 4% 的分界。
pub const MATCH_THRESHOLD: f32 = 0.045;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoTone {
    pub exposure: f64,
    pub contrast: f64,
    pub saturation: f64,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub points: Vec<[f32; 2]>,
    pub tone: AutoTone,
}

fn histogram(rgb: &[u8]) -> Option<([u32; BINS], usize, f32)> {
    if rgb.len() < 3 || !rgb.len().is_multiple_of(3) {
        return None;
    }
    let mut bins = [0u32; BINS];
    let mut sampled = 0usize;
    let mut saturation = 0f64;
    let step = (rgb.len() / 3 / 65_536).max(1);
    for pixel in rgb.as_chunks::<3>().0.iter().step_by(step) {
        let [r, g, b] = [
            pixel[0] as f32 / 255.0,
            pixel[1] as f32 / 255.0,
            pixel[2] as f32 / 255.0,
        ];
        let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0);
        bins[(luma * 255.0).round() as usize] += 1;
        let max = r.max(g).max(b);
        saturation += if max <= 0.001 {
            0.0
        } else {
            ((max - r.min(g).min(b)) / max) as f64
        };
        sampled += 1;
    }
    Some((bins, sampled, (saturation / sampled as f64) as f32))
}

fn quantile(bins: &[u32; BINS], count: usize, q: f32) -> f32 {
    let target = q * count as f32;
    let mut seen = 0u32;
    for (i, amount) in bins.iter().enumerate() {
        let next = seen + amount;
        if next as f32 >= target && *amount > 0 {
            return ((i as f32 + ((target - seen as f32) / *amount as f32).clamp(0.0, 1.0))
                / 255.0)
                .clamp(0.0, 1.0);
        }
        seen = next;
    }
    1.0
}

/// 用分位数配对消除两图尺寸、方向与轻微裁切差异。动态范围过窄时拒绝建档。
pub fn analyze(raw_rgb: &[u8], sooc_rgb: &[u8]) -> Result<Analysis, String> {
    let (raw_hist, raw_n, _) = histogram(raw_rgb).ok_or("RAW 采样像素为空或损坏")?;
    let (sooc_hist, sooc_n, sat) = histogram(sooc_rgb).ok_or("直出采样像素为空或损坏")?;
    let raw_span = quantile(&raw_hist, raw_n, 0.92) - quantile(&raw_hist, raw_n, 0.08);
    let sooc_low = quantile(&sooc_hist, sooc_n, 0.05);
    let sooc_high = quantile(&sooc_hist, sooc_n, 0.95);
    if raw_span < 0.12 || sooc_high - sooc_low < 0.12 {
        return Err("样张反差过低，无法可靠拟合基础曲线".into());
    }
    let mut points = vec![[0.0, 0.0]];
    for q in QUANTILES {
        let x = quantile(&raw_hist, raw_n, q);
        let y = quantile(&sooc_hist, sooc_n, q);
        if x > points.last().unwrap()[0] + 1.0 / 256.0 && x < 1.0 - 1.0 / 256.0 {
            points.push([x, y.max(points.last().unwrap()[1])]);
        }
    }
    points.push([1.0, 1.0]);
    Curve::from_points(points.clone())?;
    let median = quantile(&sooc_hist, sooc_n, 0.5);
    let span = sooc_high - sooc_low;
    // 轻量全局修正；将画面保留在摄影师预期附近，避免自动色阶截断高光/阴影。
    let exposure = ((((0.46_f32 / median.max(0.08)).log2() * 0.25).clamp(-0.3, 0.3) / 0.05).round()
        * 0.05) as f64;
    let contrast = ((0.72 - span) * 35.0).clamp(-12.0, 12.0).round() as f64;
    let saturation = ((0.28 - sat) * 35.0).clamp(-10.0, 10.0).round() as f64;
    Ok(Analysis {
        points,
        tone: AutoTone {
            exposure,
            contrast,
            saturation,
        },
    })
}

/// 固定 33 点亮度序列 + 邻段斜率差，减少曝光偏差造成的误归档。
pub fn distance(a: &[[f32; 2]], b: &[[f32; 2]]) -> Result<f32, String> {
    let a = Curve::from_points(a.to_vec())?;
    let b = Curve::from_points(b.to_vec())?;
    let mut value = 0.0;
    let mut slope = 0.0;
    let mut prev = (0.0, 0.0);
    for i in 0..=32 {
        let x = i as f32 / 32.0;
        let pair = (a.eval(x), b.eval(x));
        let weight = if (4..=28).contains(&i) { 1.0 } else { 0.4 };
        value += weight * (pair.0 - pair.1).powi(2);
        if i > 0 {
            slope += ((pair.0 - prev.0) - (pair.1 - prev.1)).powi(2);
        }
        prev = pair;
    }
    Ok(((value / 27.0) + (slope / 32.0) * 0.2).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ramp<F: Fn(f32) -> f32>(f: F) -> Vec<u8> {
        (0..=255)
            .flat_map(|i| {
                let v = (f(i as f32 / 255.0).clamp(0.0, 1.0) * 255.0) as u8;
                [v, v, v]
            })
            .collect()
    }
    #[test]
    fn fitting_and_matching_are_bounded() {
        let raw = ramp(|x| x);
        let neutral = ramp(|x| x);
        let warm = ramp(|x| x.powf(0.8));
        let hard = ramp(|x| x.powf(0.45));
        let a = analyze(&raw, &neutral).unwrap();
        let b = analyze(&raw, &warm).unwrap();
        let c = analyze(&raw, &hard).unwrap();
        assert!(distance(&a.points, &a.points).unwrap() < 0.001);
        assert!(distance(&a.points, &b.points).unwrap() > MATCH_THRESHOLD);
        assert!(distance(&b.points, &c.points).unwrap() > MATCH_THRESHOLD);
        assert!(
            a.tone.exposure.abs() <= 0.3
                && a.tone.contrast.abs() <= 12.0
                && a.tone.saturation.abs() <= 10.0
        );
    }
    #[test]
    fn reject_empty_and_flat_samples() {
        let ramp = ramp(|x| x);
        assert!(analyze(&[], &ramp).is_err());
        assert!(analyze(&vec![128; 768], &ramp).is_err());
        assert!(analyze(&ramp, &vec![0; 768]).is_err());
    }
}
