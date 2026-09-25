//! BM3D：块匹配 → 3D 硬阈值基础估计 → 基础估计引导的 Wiener 协同滤波。
//! DCT / Hadamard、分组与加权聚合移植自 RapidRAW `denoising.rs`，AGPL-3.0。
//! 来源 https://github.com/CyberTimon/RapidRAW （详见 THIRD-PARTY-NOTICES.md）。
//! 适配：独立亮度/色度、线性 u16、可取消、有界 tile 内存、修复边缘覆盖和 SSD 单位。
//! 无相机噪声档案，滑杆指定线性域噪声尺度；这是非 AI 的两阶段 BM3D。

use super::{
    denoise::DenoisePlan,
    pipeline::{LinearImage, luma_of},
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
const BLOCK_SIZE: usize = 8;
const MAX_GROUP_SIZE: usize = 16;
const STRIDE: usize = 6;
const SEARCH_RADIUS: usize = 9;
// 两遍聚合的影响半径 < 4*(搜索半径+块边长)，留 72 px 并对齐参考网格。
const HALO: usize = 72;
const TILE: usize = 384;

/// 取消返回 None；禁用/非法图直接保留原像素。工作线程最多八条，并为交互保留至少两条逻辑线程。
pub fn denoise_high(
    source: &LinearImage,
    plan: &DenoisePlan,
    cancel: &AtomicBool,
) -> Option<LinearImage> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    if plan.is_identity() || !source.is_consistent() {
        return Some(source.clone());
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .saturating_sub(2)
        .clamp(1, 8);
    high_with_threads(source, plan, cancel, threads, TILE)
}

fn high_with_threads(
    source: &LinearImage,
    plan: &DenoisePlan,
    cancel: &AtomicBool,
    threads: usize,
    tile_size: usize,
) -> Option<LinearImage> {
    let (w, h) = (source.width as usize, source.height as usize);
    let jobs: Vec<_> = (0..h)
        .step_by(tile_size)
        .flat_map(|y| (0..w).step_by(tile_size).map(move |x| (x, y)))
        .collect();
    let next = AtomicUsize::new(0);
    let tables = DctTables::new();
    let sigma = [
        strength(plan.luma) * 10.0,
        strength(plan.chroma) * 15.0,
        strength(plan.chroma) * 15.0,
    ];
    let mut out = source.clone();
    std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::sync_channel(threads.max(1));
        for _ in 0..threads.max(1).min(jobs.len()) {
            let sender = sender.clone();
            let (jobs, next, tables) = (&jobs, &next, &tables);
            scope.spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let Some(&(x, y)) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) else {
                        break;
                    };
                    let Some(tile) = process_tile(source, x, y, tile_size, &sigma, tables, cancel)
                    else {
                        break;
                    };
                    if sender.send((x, y, tile)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
        for (x, y, tile) in receiver {
            let tw = tile_size.min(w - x);
            for (dy, row) in tile.chunks_exact(tw * 3).enumerate() {
                let start = ((y + dy) * w + x) * 3;
                out.rgb[start..start + row.len()].copy_from_slice(row);
            }
        }
    });
    (!cancel.load(Ordering::Relaxed)).then_some(out)
}

fn strength(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn process_tile(
    source: &LinearImage,
    x: usize,
    y: usize,
    tile_size: usize,
    sigma: &[f32; 3],
    tables: &DctTables,
    cancel: &AtomicBool,
) -> Option<Vec<u16>> {
    let (sw, sh) = (source.width as usize, source.height as usize);
    let (tw, th) = (tile_size.min(sw - x), tile_size.min(sh - y));
    let (left, top) = (x.saturating_sub(HALO), y.saturating_sub(HALO));
    let (w, h) = (
        ((x + tw + HALO).min(sw) - left).max(8),
        ((y + th + HALO).min(sh) - top).max(8),
    );
    let mut channels: [Vec<f32>; 3] = std::array::from_fn(|_| vec![0.0; w * h]);
    for py in 0..h {
        for px in 0..w {
            let at = (((top + py).min(sh - 1) * sw) + (left + px).min(sw - 1)) * 3;
            let r = f32::from(source.rgb[at]) / 257.0;
            let g = f32::from(source.rgb[at + 1]) / 257.0;
            let b = f32::from(source.rgb[at + 2]) / 257.0;
            let l = luma_of([r, g, b]);
            channels[0][py * w + px] = l;
            channels[1][py * w + px] = b - l;
            channels[2][py * w + px] = r - l;
        }
    }
    let basic = step(&channels, &channels, w, h, sigma, true, tables, cancel)?;
    let result = step(&channels, &basic, w, h, sigma, false, tables, cancel)?;
    let mut rgb = Vec::with_capacity(tw * th * 3);
    for py in y - top..y - top + th {
        for px in x - left..x - left + tw {
            let i = py * w + px;
            let l = result[0][i];
            let r = l + result[2][i];
            let b = l + result[1][i];
            let g = (l - 0.2126 * r - 0.0722 * b) / 0.7152;
            rgb.extend([r, g, b].map(|v| (v * 257.0).round().clamp(0.0, 65535.0) as u16));
        }
    }
    Some(rgb)
}

fn anchors(length: usize) -> Vec<usize> {
    let end = length - BLOCK_SIZE;
    let mut values: Vec<_> = (0..=end).step_by(STRIDE).collect();
    if values.last() != Some(&end) {
        values.push(end);
    }
    values
}

#[allow(clippy::too_many_arguments)]
fn step(
    noisy: &[Vec<f32>; 3],
    guide: &[Vec<f32>; 3],
    w: usize,
    h: usize,
    sigma: &[f32; 3],
    hard: bool,
    tables: &DctTables,
    cancel: &AtomicBool,
) -> Option<[Vec<f32>; 3]> {
    let mut numerators: [Vec<f32>; 3] = std::array::from_fn(|_| vec![0.0; w * h]);
    let mut denominators: [Vec<f32>; 3] = std::array::from_fn(|_| vec![0.0; w * h]);
    let xs = anchors(w);
    let threshold =
        (sigma.iter().map(|s| s * s).sum::<f32>() * if hard { 8.0 } else { 3.0 }).max(1.0);
    for ry in anchors(h) {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        for &rx in &xs {
            let locations = block_matching(guide, w, h, rx, ry, threshold);
            for ch in 0..3 {
                if sigma[ch] == 0.0 {
                    continue;
                }
                let mut stack = build_3d_group(&noisy[ch], w, &locations);
                transform_3d(&mut stack, locations.len(), tables);
                let weight = if hard {
                    1.0 / hard_threshold(&mut stack, 2.7 * sigma[ch]).max(1) as f32
                } else {
                    let mut pilot = build_3d_group(&guide[ch], w, &locations);
                    transform_3d(&mut pilot, locations.len(), tables);
                    wiener_filter(&mut stack, &pilot, sigma[ch])
                };
                inverse_transform_3d(&mut stack, locations.len(), tables);
                for (k, &(lx, ly)) in locations.iter().enumerate() {
                    for dy in 0..8 {
                        for dx in 0..8 {
                            let pixel = (ly + dy) * w + lx + dx;
                            let at = dy * 8 + dx;
                            let weighted = weight * tables.kaiser[at];
                            numerators[ch][pixel] += stack[k * 64 + at] * weighted;
                            denominators[ch][pixel] += weighted;
                        }
                    }
                }
            }
        }
    }
    Some(std::array::from_fn(|ch| {
        numerators[ch]
            .iter()
            .zip(&denominators[ch])
            .zip(&noisy[ch])
            .map(|((&n, &d), &original)| if d > 0.0 { n / d } else { original })
            .collect()
    }))
}

/// SSD 统一为每像素三通道平方和；提前退出也必须用相同单位。
fn block_matching(
    channels: &[Vec<f32>; 3],
    w: usize,
    h: usize,
    rx: usize,
    ry: usize,
    threshold: f32,
) -> Vec<(usize, usize)> {
    let mut reference = [[0.0; 64]; 3];
    for ch in 0..3 {
        extract_patch(&channels[ch], w, rx, ry, &mut reference[ch]);
    }
    let mut candidates = vec![(0.0, rx, ry)];
    let mut visited = [[false; 2 * SEARCH_RADIUS + 1]; 2 * SEARCH_RADIUS + 1];
    visited[SEARCH_RADIUS][SEARCH_RADIUS] = true;
    let mut visit = |dx: isize, dy: isize, candidates: &mut Vec<(f32, usize, usize)>| {
        let radius = SEARCH_RADIUS as isize;
        if dx.abs() > radius || dy.abs() > radius {
            return;
        }
        let seen = &mut visited[(dy + radius) as usize][(dx + radius) as usize];
        if *seen {
            return;
        }
        *seen = true;
        let Some(x) = rx.checked_add_signed(dx) else {
            return;
        };
        let Some(y) = ry.checked_add_signed(dy) else {
            return;
        };
        if x > w - 8 || y > h - 8 {
            return;
        }
        let distance = patch_distance(channels, w, x, y, &reference, threshold);
        if distance <= threshold {
            candidates.push((distance, x, y));
        }
    };
    // 两像素粗网格，再细查最接近的四块的邻域；保留 19x19 搜索范围和真实 8x8 SSD。
    // 相比逐点穷举减少距离计算，避免 24MP 的后台等待接近分钟级。
    for dy in (-8..=8).step_by(2) {
        for dx in (-8..=8).step_by(2) {
            visit(dx, dy, &mut candidates);
        }
    }
    let compare = |a: &(f32, usize, usize), b: &(f32, usize, usize)| {
        a.0.total_cmp(&b.0)
            .then_with(|| ((b.1, b.2) == (rx, ry)).cmp(&((a.1, a.2) == (rx, ry))))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.1.cmp(&b.1))
    };
    let refine_count = 4.min(candidates.len());
    candidates.select_nth_unstable_by(refine_count - 1, compare);
    let refine: Vec<_> = candidates[..refine_count]
        .iter()
        .map(|&(_, x, y)| (x as isize - rx as isize, y as isize - ry as isize))
        .collect();
    for (x, y) in refine {
        for dy in -1..=1 {
            for dx in -1..=1 {
                visit(x + dx, y + dy, &mut candidates);
            }
        }
    }
    let count = MAX_GROUP_SIZE.min(candidates.len());
    let count = 1 << count.ilog2();
    candidates.select_nth_unstable_by(count - 1, compare);
    candidates[..count].sort_unstable_by(compare);
    candidates[..count]
        .iter()
        .map(|&(_, x, y)| (x, y))
        .collect()
}
fn patch_distance(
    channels: &[Vec<f32>; 3],
    w: usize,
    x: usize,
    y: usize,
    reference: &[[f32; 64]; 3],
    threshold: f32,
) -> f32 {
    // 八条独立累加链，允许编译器向量化；避免逐元素相加的串行浮点依赖。
    let mut sums = [0.0_f32; 8];
    let limit = threshold * 64.0;
    for ch in 0..3 {
        for dy in 0..8 {
            let start = (y + dy) * w + x;
            let row = &channels[ch][start..start + 8];
            let reference = &reference[ch][dy * 8..dy * 8 + 8];
            for dx in 0..8 {
                let delta = row[dx] - reference[dx];
                sums[dx] += delta * delta;
            }
        }
        if sums.iter().sum::<f32>() > limit {
            return f32::INFINITY;
        }
    }
    sums.iter().sum::<f32>() / 64.0
}

fn hard_threshold(stack: &mut [f32], th: f32) -> usize {
    let mut c = 0;
    for (i, x) in stack.iter_mut().enumerate() {
        if i == 0 {
            c += 1;
            continue;
        }

        if x.abs() < th {
            *x = 0.0;
        } else {
            c += 1;
        }
    }
    c
}

fn wiener_filter(noisy: &mut [f32], guide: &[f32], sigma: f32) -> f32 {
    let mut sum = 0.0;
    let s2 = sigma * sigma;
    for (i, (n, g)) in noisy.iter_mut().zip(guide).enumerate() {
        if i == 0 {
            sum += 1.0;
            continue;
        }

        let energy = g * g;
        let coef = energy / (energy + s2 + 1e-5);
        *n *= coef;
        sum += coef * coef;
    }
    if sum > 0.0 { 1.0 / sum } else { 1.0 }
}

#[inline(always)]
fn extract_patch(img: &[f32], w: usize, x: usize, y: usize, out: &mut [f32]) {
    for dy in 0..8 {
        let src_idx = (y + dy) * w + x;
        let dst_idx = dy * 8;
        out[dst_idx..dst_idx + 8].copy_from_slice(&img[src_idx..src_idx + 8]);
    }
}

fn build_3d_group(img: &[f32], w: usize, locs: &[(usize, usize)]) -> Vec<f32> {
    let mut stack = vec![0.0; locs.len() * 64];
    for (i, &(lx, ly)) in locs.iter().enumerate() {
        let offset = i * 64;
        extract_patch(img, w, lx, ly, &mut stack[offset..offset + 64]);
    }
    stack
}

struct DctTables {
    dct_coeff: [f32; 64],
    idct_coeff: [f32; 64],
    kaiser: Vec<f32>,
}

impl DctTables {
    fn new() -> Self {
        let mut dct_coeff = [0.0; 64];
        let mut idct_coeff = [0.0; 64];
        for k in 0..8 {
            for n in 0..8 {
                let c = k as f32 * std::f32::consts::PI / 8.0;
                let val = ((n as f32 + 0.5) * c).cos();
                let scale = if k == 0 {
                    std::f32::consts::FRAC_1_SQRT_2 * 0.5
                } else {
                    0.5
                };
                dct_coeff[k * 8 + n] = val * scale;
            }
        }
        for n in 0..8 {
            for k in 0..8 {
                let theta = (std::f32::consts::PI / 8.0) * (n as f32 + 0.5) * (k as f32);
                let scale = if k == 0 {
                    std::f32::consts::FRAC_1_SQRT_2 * 0.5
                } else {
                    0.5
                };
                idct_coeff[n * 8 + k] = scale * theta.cos();
            }
        }
        let mut kaiser = vec![0.0; 64];
        for y in 0..8 {
            for x in 0..8 {
                let wx = (std::f32::consts::PI * (x as f32 + 0.5) / 8.0).sin();
                let wy = (std::f32::consts::PI * (y as f32 + 0.5) / 8.0).sin();
                kaiser[y * 8 + x] = wx * wy;
            }
        }
        Self {
            dct_coeff,
            idct_coeff,
            kaiser,
        }
    }
}

#[inline(always)]
fn transform_3d(stack: &mut [f32], group_size: usize, tables: &DctTables) {
    for i in 0..group_size {
        let offset = i * 64;
        dct_2d_8x8(&mut stack[offset..offset + 64], &tables.dct_coeff);
    }
    for i in 0..64 {
        let mut col = [0.0; MAX_GROUP_SIZE];
        for k in 0..group_size {
            col[k] = stack[k * 64 + i];
        }
        walsh_hadamard_1d(&mut col[0..group_size]);
        for k in 0..group_size {
            stack[k * 64 + i] = col[k];
        }
    }
}

#[inline(always)]
fn inverse_transform_3d(stack: &mut [f32], group_size: usize, tables: &DctTables) {
    for i in 0..64 {
        let mut col = [0.0; MAX_GROUP_SIZE];
        for k in 0..group_size {
            col[k] = stack[k * 64 + i];
        }
        walsh_hadamard_1d(&mut col[0..group_size]);
        for k in 0..group_size {
            stack[k * 64 + i] = col[k];
        }
    }
    for i in 0..group_size {
        let offset = i * 64;
        idct_2d_8x8(&mut stack[offset..offset + 64], &tables.idct_coeff);
    }
}

#[inline]
fn dct_2d_8x8(block: &mut [f32], coeffs: &[f32; 64]) {
    for i in 0..8 {
        dct_1d_8(&mut block[i * 8..(i + 1) * 8], coeffs);
    }
    transpose_8x8(block);
    for i in 0..8 {
        dct_1d_8(&mut block[i * 8..(i + 1) * 8], coeffs);
    }
    transpose_8x8(block);
}

#[inline]
fn idct_2d_8x8(block: &mut [f32], coeffs: &[f32; 64]) {
    transpose_8x8(block);
    for i in 0..8 {
        idct_1d_8(&mut block[i * 8..(i + 1) * 8], coeffs);
    }
    transpose_8x8(block);
    for i in 0..8 {
        idct_1d_8(&mut block[i * 8..(i + 1) * 8], coeffs);
    }
}

#[inline]
fn dct_1d_8(x: &mut [f32], coeffs: &[f32; 64]) {
    let mut tmp = [0.0; 8];
    tmp.copy_from_slice(x);
    for (k, x_k) in x[..8].iter_mut().enumerate() {
        let mut s = 0.0;
        let row_start = k * 8;
        for (n, &tmp_n) in tmp.iter().enumerate() {
            s += tmp_n * coeffs[row_start + n];
        }
        *x_k = s;
    }
}

#[inline]
fn idct_1d_8(x: &mut [f32], coeffs: &[f32; 64]) {
    let mut tmp = [0.0; 8];
    tmp.copy_from_slice(x);
    for (n, x_n) in x[..8].iter_mut().enumerate() {
        let mut s = 0.0;
        let row_start = n * 8;
        for (k, &tmp_k) in tmp.iter().enumerate() {
            s += tmp_k * coeffs[row_start + k];
        }
        *x_n = s;
    }
}

#[inline]
fn transpose_8x8(b: &mut [f32]) {
    for y in 0..8 {
        for x in (y + 1)..8 {
            b.swap(y * 8 + x, x * 8 + y);
        }
    }
}

#[inline]
fn walsh_hadamard_1d(data: &mut [f32]) {
    let n = data.len();
    let mut h = 1;
    while h < n {
        for i in (0..n).step_by(h * 2) {
            for j in i..i + h {
                let x = data[j];
                let y = data[j + h];
                data[j] = x + y;
                data[j + h] = x - y;
            }
        }
        h *= 2;
    }
    let scale = 1.0 / (n as f32).sqrt();
    for x in data {
        *x *= scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image(w: u32, h: u32) -> (LinearImage, LinearImage) {
        let mut seed = 739_u32;
        let mut clean = LinearImage {
            width: w,
            height: h,
            rgb: Vec::new(),
        };
        let mut noisy = clean.clone();
        for _y in 0..h {
            for x in 0..w {
                let value = if x < w / 2 { 12000 } else { 42000 };
                for _ in 0..3 {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    clean.rgb.push(value);
                    noisy
                        .rgb
                        .push((i32::from(value) + ((seed >> 16) % 3001) as i32 - 1500) as u16);
                }
            }
        }
        (clean, noisy)
    }
    #[test]
    fn transforms_are_orthonormal_and_reversible_for_every_group_size() {
        let tables = DctTables::new();
        for n in [1, 2, 4, 8, 16] {
            let original: Vec<_> = (0..n * 64)
                .map(|i| ((i * 37 % 131) as f32 - 65.0) / 10.0)
                .collect();
            let mut transformed = original.clone();
            let energy = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>();
            transform_3d(&mut transformed, n, &tables);
            assert!((energy(&transformed) / energy(&original) - 1.0).abs() < 1e-5);
            inverse_transform_3d(&mut transformed, n, &tables);
            assert!(
                transformed
                    .iter()
                    .zip(&original)
                    .all(|(a, b)| (a - b).abs() < 1e-4)
            );
        }
    }
    #[test]
    fn both_stages_reduce_noise_without_erasing_the_edge() {
        let (clean, noisy) = image(32, 24);
        let out = denoise_high(
            &noisy,
            &DenoisePlan {
                luma: 0.7,
                chroma: 0.7,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        let mse = |data: &[u16]| {
            data.iter()
                .zip(&clean.rgb)
                .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
                .sum::<f64>()
                / data.len() as f64
        };
        assert!(
            mse(&out.rgb) < mse(&noisy.rgb) * 0.45,
            "{} vs {}",
            mse(&out.rgb),
            mse(&noisy.rgb)
        );
        for y in 0..24 {
            assert!(out.rgb[(y * 32 + 16) * 3] > out.rgb[(y * 32 + 15) * 3] + 20000);
        }
    }
    #[test]
    fn zero_invalid_and_cancelled_inputs() {
        let (_, source) = image(9, 7);
        assert_eq!(
            denoise_high(&source, &DenoisePlan::default(), &AtomicBool::new(false)).unwrap(),
            source
        );
        assert!(
            denoise_high(
                &source,
                &DenoisePlan {
                    luma: 1.0,
                    chroma: 1.0
                },
                &AtomicBool::new(true)
            )
            .is_none()
        );
        for source in [
            LinearImage {
                width: 0,
                height: 0,
                rgb: vec![],
            },
            LinearImage {
                width: 7,
                height: 5,
                rgb: vec![3],
            },
        ] {
            assert_eq!(
                denoise_high(
                    &source,
                    &DenoisePlan {
                        luma: 1.0,
                        chroma: 1.0
                    },
                    &AtomicBool::new(false)
                )
                .unwrap(),
                source
            );
        }
    }
    #[test]
    fn tiny_flat_images_keep_borders_and_luma() {
        for (w, h) in [(1, 1), (1, 17), (17, 1), (7, 9), (8, 8), (15, 11)] {
            let source = LinearImage {
                width: w,
                height: h,
                rgb: [23000, 31000, 17000].repeat((w * h) as usize),
            };
            let out = denoise_high(
                &source,
                &DenoisePlan {
                    luma: 1.0,
                    chroma: 1.0,
                },
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(
                out.rgb
                    .iter()
                    .zip(&source.rgb)
                    .all(|(a, b)| a.abs_diff(*b) <= 1),
                "{w}x{h}"
            );
        }
        let (_, source) = image(17, 13);
        let out = denoise_high(
            &source,
            &DenoisePlan {
                luma: 0.0,
                chroma: 0.8,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        for (a, b) in source.rgb.chunks_exact(3).zip(out.rgb.chunks_exact(3)) {
            let y = |p: &[u16]| luma_of([f32::from(p[0]), f32::from(p[1]), f32::from(p[2])]);
            assert!((y(a) - y(b)).abs() < 1.0);
        }
    }
    #[test]
    fn matching_threshold_uses_mean_squared_units_even_when_short_circuiting() {
        let channels = std::array::from_fn(|_| vec![1.0; 64]);
        assert_eq!(
            patch_distance(&channels, 8, 0, 0, &[[0.0; 64]; 3], 4.0),
            3.0
        );
        assert!(patch_distance(&channels, 8, 0, 0, &[[0.0; 64]; 3], 2.0).is_infinite());
    }
    #[test]
    fn tile_boundaries_and_thread_counts_match_one_region() {
        for (w, h) in [(211, 13), (13, 211)] {
            let (_, source) = image(w, h);
            let plan = DenoisePlan {
                luma: 0.7,
                chroma: 0.7,
            };
            let cancel = AtomicBool::new(false);
            let whole = high_with_threads(&source, &plan, &cancel, 1, 384).unwrap();
            let tiles = high_with_threads(&source, &plan, &cancel, 3, 96).unwrap();
            assert!(
                whole
                    .rgb
                    .iter()
                    .zip(&tiles.rgb)
                    .all(|(a, b)| a.abs_diff(*b) <= 1),
                "{w}x{h}: tile seam"
            );
        }
    }
}
