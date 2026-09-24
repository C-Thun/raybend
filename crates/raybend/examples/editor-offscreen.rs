//! 编辑视口的**离屏像素证据**（M3-W2）：不需要窗口，也能断言「照片真的按视口摆好了」。
//!
//! ```bash
//! cargo run -p raybend --example editor-offscreen -- /mnt/c/src/tmp/raybend-w2
//! ```
//!
//! 为什么需要它（不是「顺手写的 demo」）：编辑视口跑在 Windows 真机的窗口里，
//! 而本机 WSL 只有 lavapipe、开不了 Tauri 窗口 —— 「管线通不通、坐标对不对、底色是什么色」
//! 在交给人类之前**完全无法验证**。这个例子用同一条管线（同一个 `OffscreenRenderer`、
//! 同一份 shader 与矩阵）把一张**合成照片**画出来并回读像素。
//!
//! # 期望值全部是**外部给定**的（这是这个例子唯一的价值所在）
//!
//! `docs/native-viewport-coordinate-guide.md` §6 第 3 条点名过这件事：
//! 判据里必须含**外部给定的像素位置与颜色**，不能「让被测函数自己生成期望值」——
//! 后者只能证明自洽（往返误差 0 也证不了单位对）。
//!
//! 所以下面每个断言的**坐标与颜色都是手算好写在代码里的常量**：
//!
//! | 断言 | 外部给定的依据 |
//! | --- | --- |
//! | 四块颜色落在正确的象限 | 合成图是我们自己按 2×2 拼的，颜色是常量 |
//! | 照片之外是**洞口底色**且**不透明** | 「报 40/44/52 就该回读到 40/44/52」—— 这条钉住 wgpu 在 sRGB 目标上把清屏值当**线性值**（第一次跑就靠它发现少了 sRGB→线性那一步） |
//! | 1:1 下块边界正好落在图心 | 1:1 = 一个图像像素一个物理像素，偏移 1 像素就失去意义 |
//! | 洞口之外一个像素都不碰（scissor） | 洞口矩形是常量，断言点贴着它外面一格 |
//!
//! 落盘的 PNG 给人看（`AGENTS.md` §2.8：像素级的目视判断归人类）。

use std::path::PathBuf;

use image::{Rgba, RgbaImage};
use raybend::render::color::Srgb8;
use raybend::render::viewport::{ClipRect, FitMode, Viewport};
use raybend::render::{OffscreenRenderer, RenderImage};

/// 合成照片：2×2 四块，颜色是**常量**（断言里直接写它们）。
///
/// 为什么不用真实照片：真实照片的期望值只能靠「上次跑出来是什么」——
/// 那就退回自洽了；合成图的每一点颜色都是我们自己放进去的。
const QUADRANTS: [[u8; 4]; 4] = [
    [200, 10, 10, 255],  // 左上：红
    [10, 200, 10, 255],  // 右上：绿
    [10, 10, 200, 255],  // 左下：蓝
    [240, 240, 10, 255], // 右下：黄
];

/// 每块 32×32（整张 64×64）—— 够大，重采样不会把块中心与边界搅在一起。
const BLOCK: u32 = 32;

/// 洞口底色（外部给定的 sRGB 值，就取深色主题的 `--surface-bar`）。
const BACKDROP: Srgb8 = Srgb8 {
    r: 40,
    g: 44,
    b: 52,
};

fn make_photo() -> RenderImage {
    let size = BLOCK * 2;
    let mut rgb = Vec::with_capacity((size * size * 3) as usize);
    for y in 0..size {
        for x in 0..size {
            let quadrant = usize::from(x >= BLOCK) + usize::from(y >= BLOCK) * 2;
            rgb.extend_from_slice(&expected(quadrant));
        }
    }
    RenderImage::from_rgb8(size, size, &rgb).expect("合成的照片长度必须对得上")
}

/// **缩到一半的纹理**（32×32，每块 16×16）—— 验「1:1 按逻辑尺寸算」时替掉全尺寸纹理。
fn make_half_photo() -> RenderImage {
    let size = BLOCK; // 32
    let half = BLOCK / 2; // 16
    let mut rgb = Vec::with_capacity((size * size * 3) as usize);
    for y in 0..size {
        for x in 0..size {
            let quadrant = usize::from(x >= half) + usize::from(y >= half) * 2;
            rgb.extend_from_slice(&expected(quadrant));
        }
    }
    RenderImage::from_rgb8(size, size, &rgb).expect("缩略纹理的长度必须对得上")
}

/// 第 `quadrant` 块的 RGB（外部给定的期望色，从 [`QUADRANTS`] 里取）。
fn expected(quadrant: usize) -> [u8; 3] {
    let [r, g, b, _] = QUADRANTS[quadrant];
    [r, g, b]
}

fn rgba(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let index = ((y as usize) * (width as usize) + x as usize) * 4;
    [
        pixels[index],
        pixels[index + 1],
        pixels[index + 2],
        pixels[index + 3],
    ]
}

/// 颜色是否大致等于期望（容差留给重采样；alpha 必须不透明）。
fn near(actual: [u8; 4], expected: [u8; 3], tolerance: i32) -> bool {
    if actual[3] < 250 {
        return false;
    }
    (0..3).all(|i| (i32::from(actual[i]) - i32::from(expected[i])).abs() <= tolerance)
}

fn is_backdrop(actual: [u8; 4]) -> bool {
    near(actual, [BACKDROP.r, BACKDROP.g, BACKDROP.b], 2)
}

fn save_png(path: &PathBuf, pixels: &[u8], width: u32, height: u32) -> Result<(), String> {
    let mut image = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let [r, g, b, a] = rgba(pixels, width, x, y);
            image.put_pixel(x, y, Rgba([r, g, b, a]));
        }
    }
    image.save(path).map_err(|e| format!("写 PNG 失败：{e}"))
}

fn main() -> Result<(), String> {
    let out_dir: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/raybend-editor-offscreen".to_string())
        .into();
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("建输出目录失败：{e}"))?;

    let photo = make_photo();
    let mut renderer = OffscreenRenderer::with_image(None, photo, "editor-offscreen")
        .map_err(|e| format!("建离屏渲染器失败（本环境有适配器吗）：{e}"))?;
    let (image_w, image_h) = (renderer.image().width, renderer.image().height);
    println!("合成照片：{image_w}×{image_h}（四块 32×32），洞口底色 = {BACKDROP:?}");

    let mut failures: Vec<String> = Vec::new();
    let mut check = |what: &str, ok: bool, detail: String| {
        if ok {
            println!("  ✓ {what}");
            return;
        }
        println!("  ✗ {what} —— {detail}");
        failures.push(format!("{what}：{detail}"));
    };

    renderer.set_backdrop(BACKDROP.to_clear_color());

    /* ── ① 适合窗口（Fit）：照片居中、两侧留出洞口底色 ─────────────
     *
     * 窗口 128×96、照片 64×64 ⇒ Fit 倍率 = min(128/64, 96/64) = 1.5
     * ⇒ 照片占 x ∈ [16,112]、y ∈ [0,96]；四块的屏幕中心手算如下。
     */
    let size = (128u32, 96u32);
    let mut viewport = Viewport {
        image_size: (image_w, image_h),
        viewport_size: (size.0 as f32, size.1 as f32),
        fit_mode: FitMode::Fit,
        ..Default::default()
    };
    viewport.refit();
    let framed = renderer.render(&viewport, size);
    println!("\n① Fit（窗口 128×96，照片放大 1.5 倍居中）：");
    check(
        "倍率是 1.5（照片宽 96）",
        (viewport.zoom - 1.5).abs() < 1e-4,
        format!("zoom = {}", viewport.zoom),
    );
    check(
        "左上块（屏幕 40,24）是红",
        near(rgba(&framed, size.0, 40, 24), expected(0), 12),
        format!("{:?}", rgba(&framed, size.0, 40, 24)),
    );
    check(
        "右上块（屏幕 88,24）是绿",
        near(rgba(&framed, size.0, 88, 24), expected(1), 12),
        format!("{:?}", rgba(&framed, size.0, 88, 24)),
    );
    check(
        "左下块（屏幕 40,72）是蓝",
        near(rgba(&framed, size.0, 40, 72), expected(2), 12),
        format!("{:?}", rgba(&framed, size.0, 40, 72)),
    );
    check(
        "右下块（屏幕 88,72）是黄",
        near(rgba(&framed, size.0, 88, 72), expected(3), 12),
        format!("{:?}", rgba(&framed, size.0, 88, 72)),
    );
    for x in [8u32, 120] {
        check(
            &format!("左/右留白（屏幕 {x},48）是洞口底色，且不透明"),
            is_backdrop(rgba(&framed, size.0, x, 48)),
            format!(
                "{:?}（期望 {:?}）",
                rgba(&framed, size.0, x, 48),
                [BACKDROP.r, BACKDROP.g, BACKDROP.b, 255]
            ),
        );
    }
    save_png(&out_dir.join("01-fit.png"), &framed, size.0, size.1)?;

    /* ── ② 洞口裁切：洞口之内照画、洞口之外一个像素都不碰 ─────────────
     *
     * 洞口取左上 64×64；`Fill` 倍率 = max(128/64, 96/64) = 2 ⇒ 照片 128×128 居中，
     * 横向溢出：洞口里看到的是照片左上那一片（红块占据 0..64 × 0..64 的一部分）。
     */
    let mut holed = Viewport {
        image_size: (image_w, image_h),
        viewport_size: (size.0 as f32, size.1 as f32),
        clip_rect: Some(ClipRect {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
        }),
        fit_mode: FitMode::Fill,
        ..Default::default()
    };
    holed.refit();
    let clipped = renderer.render(&holed, size);
    println!("\n② 洞口（左上 64×64，照片放大 2 倍溢出）：");
    check(
        "洞口里画着照片（16,16 是红块）",
        near(rgba(&clipped, size.0, 16, 16), expected(0), 12),
        format!("{:?}", rgba(&clipped, size.0, 16, 16)),
    );
    check(
        "洞口右边界外一格（68,16）是底色",
        is_backdrop(rgba(&clipped, size.0, 68, 16)),
        format!("{:?}", rgba(&clipped, size.0, 68, 16)),
    );
    check(
        "洞口下边界外一格（16,68）是底色",
        is_backdrop(rgba(&clipped, size.0, 16, 68)),
        format!("{:?}", rgba(&clipped, size.0, 16, 68)),
    );
    save_png(&out_dir.join("02-hole.png"), &clipped, size.0, size.1)?;

    /* ── ③ 1:1：一个图像像素就是一个物理像素 ──────────────────────
     *
     * 窗口 128×96、照片 64×64、倍率 1 ⇒ 照片占 x ∈ [32,96]、y ∈ [16,80]。
     * 图心 = (64,48)，块边界正好在图心（水平 64、垂直 48）——
     * 紧挨着图心的四个像素各自属于四个不同的块，这一条同时验了
     * 「1:1 对齐」与「象限没搞反（上下、左右都不镜像）」。
     */
    let mut one_to_one = Viewport {
        image_size: (image_w, image_h),
        viewport_size: (size.0 as f32, size.1 as f32),
        fit_mode: FitMode::OneToOne,
        ..Default::default()
    };
    one_to_one.refit();
    let actual = renderer.render(&one_to_one, size);
    println!("\n③ 1:1（照片 64×64 居中，图心 = 64,48）：");
    for (x, y, quadrant, name) in [
        (63u32, 47u32, 0usize, "图心左上"),
        (65, 47, 1, "图心右上"),
        (63, 49, 2, "图心左下"),
        (65, 49, 3, "图心右下"),
    ] {
        check(
            &format!("{name}（{x},{y}）· {:?}", expected(quadrant)),
            near(rgba(&actual, size.0, x, y), expected(quadrant), 12),
            format!("{:?}", rgba(&actual, size.0, x, y)),
        );
    }
    check(
        "照片上方（64,8）是底色",
        is_backdrop(rgba(&actual, size.0, 64, 8)),
        format!("{:?}", rgba(&actual, size.0, 64, 8)),
    );
    check(
        "照片下方（64,88）是底色",
        is_backdrop(rgba(&actual, size.0, 64, 88)),
        format!("{:?}", rgba(&actual, size.0, 64, 88)),
    );
    save_png(&out_dir.join("03-one-to-one.png"), &actual, size.0, size.1)?;

    /* ── ④ 逻辑尺寸 ≠ 纹理尺寸：1:1 按**原图**算，不按当前档位 ───────────
     *
     * 这是 M3-W4 修的那条：预览档的纹理是缩小的（长边 1920），而「1:1」
     * 必须是**原图**的一个像素对一个物理像素。这里把纹理换成 32×32
     * （每块 16×16），逻辑尺寸仍是 64×64：
     * ⇒ 1:1 时照片必须占 64×64 物理像素（x ∈ [32,96]、y ∈ [16,80]），
     *   而不是纹理的 32×32（x ∈ [48,80]、y ∈ [32,64]）。
     * 四个块中心仍应是四色（最近邻放大 2 倍，块边界正好落在 32 的倍数上）。
     */
    renderer.set_image(make_half_photo());
    let mut logical = Viewport {
        image_size: (image_w, image_h), // 逻辑尺寸 = 原图 64×64
        viewport_size: (size.0 as f32, size.1 as f32),
        fit_mode: FitMode::OneToOne,
        ..Default::default()
    };
    logical.refit();
    let downscaled = renderer.render(&logical, size);
    println!("\n④ 逻辑尺寸 ≠ 纹理尺寸（纹理 32×32、逻辑 64×64）：");
    check(
        "照片边界按逻辑尺寸：左边界外一格（30,40）是底色",
        is_backdrop(rgba(&downscaled, size.0, 30, 40)),
        format!("{:?}", rgba(&downscaled, size.0, 30, 40)),
    );
    check(
        "照片边界按逻辑尺寸：右边界外一格（97,40）是底色",
        is_backdrop(rgba(&downscaled, size.0, 97, 40)),
        format!("{:?}", rgba(&downscaled, size.0, 97, 40)),
    );
    for (x, y, quadrant, name) in [
        (40u32, 24u32, 0usize, "左上块中心"),
        (88, 24, 1, "右上块中心"),
        (40, 72, 2, "左下块中心"),
        (88, 72, 3, "右下块中心"),
    ] {
        check(
            &format!("{name}（{x},{y}）· {:?}", expected(quadrant)),
            near(rgba(&downscaled, size.0, x, y), expected(quadrant), 12),
            format!("{:?}", rgba(&downscaled, size.0, x, y)),
        );
    }
    save_png(&out_dir.join("04-logical-size.png"), &downscaled, size.0, size.1)?;
    renderer.set_image(make_photo()); // 还原，别把状态留给后来的人

    println!("\nPNG 已写入 {}", out_dir.display());
    if failures.is_empty() {
        println!("全部断言通过。");
        return Ok(());
    }
    Err(format!(
        "{} 条断言没过：\n- {}",
        failures.len(),
        failures.join("\n- ")
    ))
}
