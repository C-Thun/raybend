//! **创意 LUT 上 GPU 的像素证据**：同一条 V2 显影配方，CPU 与 GPU 逐点对照。
//!
//! ```bash
//! cargo run -p raybend --example lut-offscreen
//! ```
//!
//! 为什么需要它：LUT 原来在 CPU 的 `render_working_reference` 里逐像素求值，
//! 现在改由 `spike.wgsl` 在显示参考的编码域里做（硬件三线性）。「两者是不是同一张图」
//! 只能靠**同一份输入、同一份参数、同一只 LUT** 跑两条路再逐点比 —— 肉眼看不出来。
//!
//! 判据（外部给定，不是自洽）：
//!
//! 1. 与 CPU 参考的逐点差 ≤ `TOLERANCE`（8bit 值）：允许 f16 表 + 矩阵末位 + 传输曲线近似；
//! 2. 关掉 LUT 时 GPU 与 CPU 仍然一致（证明「关闭」这条路没被改动）；
//! 3. GPU 开 LUT 与关 LUT **必须明显不同**（证明真的在着色器里套了，而不是悄悄跳过）；
//! 4. 含 1D shaper 的 LUT 同样一致（shaper 走的是另一张纹理）。
//!
//! 本机（WSL）跑的是 lavapipe 也能过：判据只看像素，不看帧率。

use raybend::color::icc::srgb_icc;
use raybend::color::working::WorkingImage;
use raybend::develop::lut::Lut;
use raybend::develop::working::{
    render_working_reference, WorkingReferenceImage, WorkingTonePlan,
};
use raybend::develop::{CurveSet, DevelopParams, DevelopStages};
use raybend::develop::denoise::NrMethod;
use raybend::render::working_preview::WorkingPreview;
use raybend::render::viewport::FitMode;
use raybend::render::{OffscreenRenderer, RenderImage, Viewport};
use std::sync::Arc;

/// 8bit 上允许的最大差。f16 存表 + 三线性 + sRGB 传输曲线的近似都在这个预算内；
/// 超过它就不是「精度」而是「配方不同」。
const TOLERANCE: i32 = 3;

const WIDTH: u32 = 48;
const HEIGHT: u32 = 48;

/// 合成图：三个方向的渐变 + 四角极值（含一颗 > 1 的高光，专门验超域残差）。
///
/// 值是 **Rec.2020 线性**（`WorkingImage::new` 的口径），允许负值与大于 1。
fn make_source() -> WorkingReferenceImage {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let u = x as f32 / (WIDTH - 1) as f32;
            let v = y as f32 / (HEIGHT - 1) as f32;
            pixels.push([u, v, 1.0 - 0.5 * (u + v)]);
        }
    }
    // 四角与中心：黑、白、过曝、偏色、负值（化验 `eval_extended` 的残差通道）。
    let corners = [
        (0_usize, [0.0, 0.0, 0.0]),
        (WIDTH as usize - 1, [0.0, 0.0, 0.0]),
        ((HEIGHT as usize - 1) * WIDTH as usize, [0.0, 0.0, 0.0]),
        (
            (WIDTH * HEIGHT) as usize - 1,
            [0.0, 0.0, 0.0],
        ),
    ];
    for (index, _) in corners {
        let x = index % WIDTH as usize;
        let y = index / WIDTH as usize;
        let value = match (x, y) {
            (0, 0) => [0.0, 0.0, 0.0],
            (x, 0) if x == WIDTH as usize - 1 => [1.6, 1.2, 1.9],
            (0, y) if y == HEIGHT as usize - 1 => [0.02, 0.5, 0.9],
            _ => [0.8, 0.1, 0.35],
        };
        pixels[index] = value;
    }
    // 中心一颗负值像素：证明残差不是「夹到 0 就算」。
    let center = (HEIGHT as usize / 2) * WIDTH as usize + WIDTH as usize / 2;
    pixels[center] = [-0.05, 0.95, 1.4];
    WorkingReferenceImage::from_working(
        WorkingImage::new(WIDTH, HEIGHT, pixels).expect("合成图尺寸必须自洽"),
    )
    .expect("工作图必须有限")
}

/// 一只非恒等的 3D LUT：交叉通道 + 反相蓝，处处都能看出「套过」。
fn cube_lut() -> Lut {
    let n = 7;
    let mut text = format!("LUT_3D_SIZE {n}\n");
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let (rf, gf, bf) = (
                    r as f32 / (n - 1) as f32,
                    g as f32 / (n - 1) as f32,
                    b as f32 / (n - 1) as f32,
                );
                let out = [
                    rf.powf(0.85) * 0.9 + 0.08 * bf,
                    gf * 0.85 + 0.1 * rf + 0.05,
                    1.0 - 0.85 * (1.0 - bf),
                ];
                text.push_str(&format!("{:.6} {:.6} {:.6}\n", out[0], out[1], out[2]));
            }
        }
    }
    Lut::parse_cube(&text).expect("构造的 cube 必须能解析")
}

/// 带 1D shaper 的 LUT：走的是第二张纹理（`lut_shaper`），单独验一遍。
fn shaper_lut() -> Lut {
    let shaper = 16;
    let n = 5;
    let mut text = format!("LUT_1D_SIZE {shaper}\n");
    for i in 0..shaper {
        let v = i as f32 / (shaper - 1) as f32;
        text.push_str(&format!("{:.6} {:.6} {:.6}\n", v.sqrt(), v.sqrt(), v));
    }
    text.push_str(&format!("LUT_3D_SIZE {n}\n"));
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let (rf, gf, bf) = (
                    r as f32 / (n - 1) as f32,
                    g as f32 / (n - 1) as f32,
                    b as f32 / (n - 1) as f32,
                );
                text.push_str(&format!(
                    "{:.6} {:.6} {:.6}\n",
                    1.0 - gf, rf * 0.7 + bf * 0.3, bf
                ));
            }
        }
    }
    Lut::parse_cube(&text).expect("构造的 shaper cube 必须能解析")
}

fn params(tone: bool) -> DevelopParams {
    let mut params = DevelopParams::new(Some(5600.0));
    if tone {
        params.set("exposure", 0.3).unwrap();
        params.set("contrast", 18.0).unwrap();
        params.set("highlights", -12.0).unwrap();
        params.set("saturation", 6.0).unwrap();
    }
    params
}

/// CPU 参考：与编辑器慢速路完全同一条配方。
fn cpu_reference(
    source: &WorkingReferenceImage,
    params: &DevelopParams,
    curves: &CurveSet,
    lut: Option<&Lut>,
) -> Vec<u8> {
    let image = render_working_reference(
        source,
        &WorkingTonePlan::new(params, curves),
        &DevelopStages {
            lut,
            ..Default::default()
        },
    )
    .expect("CPU 参考必须成功");
    let srgb = srgb_icc().expect("内置 sRGB 配置必须可用");
    image
        .to_rgb16_flat(&srgb)
        .expect("工作图必须能转成 sRGB 8bit")
        .into_iter()
        .map(|value| ((u32::from(value) + 128) / 257) as u8)
        .collect()
}

/// GPU：走编辑器的快速路（`WorkingPreview::render` → 帧自带 `gpu_lut`），
/// 再用与真窗口同一个 `OffscreenRenderer` 画出来回读。
fn gpu_render(
    source: WorkingReferenceImage,
    params: &DevelopParams,
    curves: &CurveSet,
    lut: &Arc<Lut>,
    apply: bool,
) -> Vec<u8> {
    let mut preview = WorkingPreview::new(source);
    let frame = preview
        .render(
            0,
            params,
            curves,
            NrMethod::Fast,
            None,
            if apply { Some(lut) } else { None },
            None,
        )
        .expect("快速路必须成功");
    assert!(frame.tone.is_some(), "快速路应当带影调表");
    if apply {
        assert!(frame.gpu_lut.is_some(), "启用 LUT 时帧里必须带 gpu_lut");
    }
    let mut renderer = OffscreenRenderer::with_image(
        None,
        RenderImage::clone(&frame.image),
        "lut-offscreen",
    )
    .expect("离屏渲染器需要能拿到适配器（本环境有吗）");
    renderer.set_global_tone(frame.tone.clone());
    renderer.set_creative_lut(frame.gpu_lut.clone());
    let (width, height) = (frame.image.width, frame.image.height);
    let mut viewport = Viewport {
        image_size: (width, height),
        viewport_size: (width as f32, height as f32),
        fit_mode: FitMode::Fit,
        ..Default::default()
    };
    viewport.refit();
    let rgba = renderer.render(&viewport, (width, height));
    rgba.chunks(4).flat_map(|pixel| [pixel[0], pixel[1], pixel[2]]).collect()
}

fn compare(label: &str, gpu: &[u8], cpu: &[u8]) -> (i32, i32) {
    assert_eq!(gpu.len(), cpu.len(), "{label}: 像素数量对不上");
    let mut max = 0;
    let mut over = 0;
    let mut sum = 0_i64;
    for (a, b) in gpu.iter().zip(cpu.iter()) {
        let diff = (i32::from(*a) - i32::from(*b)).abs();
        sum += i64::from(diff);
        max = max.max(diff);
        if diff > TOLERANCE {
            over += 1;
        }
    }
    let mean = sum as f64 / gpu.len() as f64;
    println!("  {label:<44} max={max:>3}  mean={mean:>5.2}  超差通道={over}");
    (max, over)
}

fn main() -> Result<(), String> {
    let source = make_source();
    let curves = CurveSet::identity();
    let cube = Arc::new(cube_lut());
    let shaper = Arc::new(shaper_lut());
    let mut failures = Vec::new();

    for tone in [false, true] {
        let params = params(tone);
        println!(
            "\n=== 全局影调 {} ===",
            if tone { "开（曝光/对比/高光/饱和）" } else { "关" }
        );
        let cpu_none = cpu_reference(&source, &params, &curves, None);
        let cpu_cube = cpu_reference(&source, &params, &curves, Some(&cube));
        let cpu_shaper = cpu_reference(&source, &params, &curves, Some(&shaper));

        let gpu_none = gpu_render(source.clone(), &params, &curves, &cube, false);
        let gpu_cube = gpu_render(source.clone(), &params, &curves, &cube, true);
        let gpu_shaper = gpu_render(source.clone(), &params, &curves, &shaper, true);

        let (max, over) = compare("关 LUT：GPU vs CPU", &gpu_none, &cpu_none);
        if max > TOLERANCE || over > 0 {
            failures.push(format!("关 LUT 时 GPU 与 CPU 不一致（max={max}，超差={over}）"));
        }
        let (max, over) = compare("cube LUT：GPU vs CPU", &gpu_cube, &cpu_cube);
        if max > TOLERANCE || over > 0 {
            failures.push(format!("cube LUT 的 GPU 与 CPU 不一致（max={max}，超差={over}）"));
        }
        let (max, over) = compare("shaper+cube：GPU vs CPU", &gpu_shaper, &cpu_shaper);
        if max > TOLERANCE || over > 0 {
            failures.push(format!(
                "shaper+cube 的 GPU 与 CPU 不一致（max={max}，超差={over}）"
            ));
        }
        // 证明确实套了 LUT：GPU 开/关 必须明显不同。
        let (_, over) = compare("自检：GPU 开 LUT vs 关 LUT", &gpu_cube, &gpu_none);
        if over == 0 {
            failures.push("GPU 开 LUT 与关 LUT 输出相同 —— 着色器里其实没套".into());
        }
    }

    if failures.is_empty() {
        println!("\n全部通过：GPU 创意 LUT 与 CPU 参考一致（容差 {TOLERANCE}/255）。");
        Ok(())
    } else {
        for failure in &failures {
            println!("  ✗ {failure}");
        }
        Err(format!("{} 项不通过", failures.len()))
    }
}
