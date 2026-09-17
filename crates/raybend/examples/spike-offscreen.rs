//! 离屏渲染冒烟：**在不需要窗口的环境里**证明「wgpu 真把图按视口变换画出来了」。
//!
//! ```bash
//! cargo run -p raybend --example spike-offscreen -- /tmp/raybend-spike
//! ```
//!
//! 为什么需要它（不是「顺手写的 demo」）：
//!
//! - `GpuContext` 必须挂在**真窗口**上，而本机 WSL 只有 lavapipe（软件 Vulkan），
//!   开不出 Tauri 窗口 —— 于是「管线通不通、变换对不对」在交接给人类之前**完全无法验证**；
//! - 这个例子用离屏路径把同样的管线、同样的矩阵跑一遍，**回读像素做断言**，
//!   再加上落盘 PNG（人也能看）。
//!
//! 它验的是**判定得了的**那部分：坐标变换与管线；人眼才判的那部分（透明合成、
//! 1:1 锐利度、跨屏）留给 `plans/M2-W1-windows-gpu.md`。

use std::path::PathBuf;

use raybend::render::viewport::{ClipRect, FitMode, Viewport};
use raybend::render::OffscreenRenderer;


fn rgba(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let index = ((y as usize) * (width as usize) + x as usize) * 4;
    [
        pixels[index],
        pixels[index + 1],
        pixels[index + 2],
        pixels[index + 3],
    ]
}

/// 判断颜色大致等于某色（容差给 sRGB 往返与重采样留余地）。
fn near(actual: [u8; 4], expected: [u8; 3], tolerance: i32) -> bool {
    if actual[3] < 200 {
        return false;
    }
    (0..3).all(|i| (actual[i] as i32 - expected[i] as i32).abs() <= tolerance)
}

fn main() -> Result<(), String> {
    let out_dir: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/raybend-spike".to_string())
        .into();
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("建输出目录失败：{e}"))?;

    let mut renderer = OffscreenRenderer::new(None).map_err(|e| e.to_string())?;
    let (image_w, image_h) = (renderer.image().width, renderer.image().height);
    println!("适配器：{:?}", renderer.adapter_info().backend);
    println!("测试图：{image_w}×{image_h}");

    let size = (800u32, 600u32);
    let mut failures: Vec<String> = Vec::new();

    /* ── ① 适配（Fit）────────────────────────────────────── */
    let mut viewport = Viewport {
        image_size: (image_w, image_h),
        viewport_size: (size.0 as f32, size.1 as f32),
        fit_mode: FitMode::Fit,
        dpr: 1.0,
        ..Default::default()
    };
    viewport.refit();
    let fit = renderer.render(&viewport, size);
    write_png(&out_dir.join("01-fit.png"), &fit, size)?;
    println!(
        "① Fit：zoom={:.4}（计算值 {:.4}）",
        viewport.zoom,
        (size.0 as f32 / image_w as f32).min(size.1 as f32 / image_h as f32)
    );

    /*
     * 图心那 8×8 白块必须落在画布中心。
     *
     * ⚠️ 这里**不能**断言「恰好纯白」：Fit 档 zoom≈0.133，8×8 的白块只有约 **1 个屏幕像素**，
     * 线性缩小自然把它和周围的十字/渐变平均掉（实测 [211,211,211]）—— 那是**正确**的重采样，
     * 不是几何错。所以这里断言「中心明显更亮」，把「像素级纯白」交给 1:1 那一组（见 ②）。
     */
    let center = rgba(&fit, size.0, size.0 / 2, size.1 / 2);
    if !(center[0] > 180 && center[1] > 180 && center[2] > 180) {
        failures.push(format!("Fit：画布中心应当明显发亮（白块所在），实测 {center:?}"));
    }
    // 上下留白（letterbox）必须是**完全透明**——出图之外一个像素都不许碰
    let top = rgba(&fit, size.0, size.0 / 2, 2);
    if top[3] != 0 {
        failures.push(format!("Fit：上留白应当完全透明（alpha=0），实测 {top:?}"));
    }
    // 四角方位标记：Fit 之后图占满宽度，左上角标记在画面左侧约 1/6 处
    let zoom = viewport.zoom;
    let mark_center_x = 400.0 + (60.0 - image_w as f32 / 2.0) * zoom;
    let mark_center_y = 300.0 + (60.0 - image_h as f32 / 2.0) * zoom;
    let tl = rgba(&fit, size.0, mark_center_x as u32, mark_center_y as u32);
    if !near(tl, [220, 40, 40], 40) {
        failures.push(format!("Fit：左上标记处应当是红色，实测 {tl:?}"));
    }
    let br_x = 400.0 + (image_w as f32 - 60.0 - image_w as f32 / 2.0) * zoom;
    let br_y = 300.0 + (image_h as f32 - 60.0 - image_h as f32 / 2.0) * zoom;
    let br = rgba(&fit, size.0, br_x as u32, br_y as u32);
    if !near(br, [60, 90, 230], 40) {
        failures.push(format!("Fit：右下标记处应当是蓝色，实测 {br:?}"));
    }

    /* ── ② 1:1 + 洞口（挖洞的语义）───────────────────────── */
    let hole = ClipRect {
        x: 100.0,
        y: 80.0,
        width: 400.0,
        height: 300.0,
    };
    let mut holed = Viewport {
        image_size: (image_w, image_h),
        viewport_size: (size.0 as f32, size.1 as f32),
        fit_mode: FitMode::OneToOne,
        clip_rect: Some(hole),
        dpr: 1.0,
        ..Default::default()
    };
    holed.refit();
    let clipped = renderer.render(&holed, size);
    write_png(&out_dir.join("02-hole-1to1.png"), &clipped, size)?;
    println!("② 1:1 + 洞口：zoom={:.4}", holed.zoom);

    // 洞口中心必须是白块（图心正好在洞口中心）
    let hole_center = rgba(&clipped, size.0, 300, 230);
    if !near(hole_center, [255, 255, 255], 12) {
        failures.push(format!("洞口中心应当是白块，实测 {hole_center:?}"));
    }
    // **洞口之外必须一个像素都没画**（这正是挖洞：外面留给 webview）
    for (x, y, what) in [
        (20u32, 20u32, "洞口左上外"),
        (20, 580, "洞口左下外"),
        (780, 20, "洞口右上外"),
        (780, 580, "洞口右下外"),
        (400, 20, "洞口上边外"),
    ] {
        let pixel = rgba(&clipped, size.0, x, y);
        if pixel[3] != 0 {
            failures.push(format!("{what}（{x},{y}）应当完全透明，实测 {pixel:?}"));
        }
    }
    // 洞口内靠近边缘处应当有内容（不是整块空白）
    let inside = rgba(&clipped, size.0, 120, 100);
    if inside[3] == 0 {
        failures.push("洞口内边角处应当有内容（alpha>0）".to_string());
    }

    /* ── ③ 旋转 90°（方位不能错）─────────────────────────── */
    let mut rotated = viewport;
    rotated.rotation = 90.0;
    let rot = renderer.render(&rotated, size);
    write_png(&out_dir.join("03-rot90.png"), &rot, size)?;
    println!("③ 旋转 90°：zoom={:.4}", rotated.zoom);
    // 绕中心旋转：图的中心点必须**不动**（旋转是绕中心做的）—— 同 ①，缩放下取「明显发亮」口径
    let center = rgba(&rot, size.0, size.0 / 2, size.1 / 2);
    if !(center[0] > 100 && center[1] > 100 && center[2] > 100) {
        failures.push(format!("旋转后中心应当仍是白块附近（绕中心旋转），实测 {center:?}"));
    }
    // 顺时针转 90°：图像**左**边缘跑到上方。左边缘中点在图像坐标 (0, ih/2)，
    // 相对中心是 (−iw/2, 0) → 转 90° 后是 (0, −iw/2) → 屏幕上方
    let left_mid_x = 400.0;
    let left_mid_y = 300.0 - (image_w as f32 / 2.0) * zoom;
    if left_mid_y < 5.0 {
        // 这个缩放下会跑出画布 —— 那本身是预期行为，换个判据：顶部中央应当有内容
        let top_center = rgba(&rot, size.0, 400, 6);
        if top_center[3] == 0 {
            failures.push("旋转 90° 后顶部中央应当有内容（图被转到竖直）".to_string());
        }
    } else {
        let pixel = rgba(&rot, size.0, left_mid_x as u32, left_mid_y as u32);
        if pixel[3] == 0 {
            failures.push(format!("旋转 90° 后上方应当有图，实测 {pixel:?}"));
        }
    }

    /* ── ④ 铺满（Fill）──────────────────────────────────── */
    let mut filled = viewport;
    filled.fit_mode = FitMode::Fill;
    filled.refit();
    let fill = renderer.render(&filled, size);
    write_png(&out_dir.join("04-fill.png"), &fill, size)?;
    println!("④ Fill：zoom={:.4}", filled.zoom);
    // Fill 不留白：四角都应当有内容
    for (x, y) in [(2u32, 2u32), (797, 2), (2, 597), (797, 597)] {
        let pixel = rgba(&fill, size.0, x, y);
        if pixel[3] == 0 {
            failures.push(format!("Fill 时 ({x},{y}) 不该留白，实测 {pixel:?}"));
        }
    }

    /* ── 汇总 ───────────────────────────────────────────── */
    println!("\nPNG 写到：{}", out_dir.display());
    if failures.is_empty() {
        println!("✅ 离屏冒烟全部通过（{} 项断言组）", 4);
        Ok(())
    } else {
        for failure in &failures {
            eprintln!("❌ {failure}");
        }
        Err(format!("{} 项断言失败", failures.len()))
    }
}

fn write_png(path: &std::path::Path, pixels: &[u8], size: (u32, u32)) -> Result<(), String> {
    if pixels.is_empty() {
        return Err(format!("没有回读到像素，写不了 {}", path.display()));
    }
    let buffer = image::RgbaImage::from_raw(size.0, size.1, pixels.to_vec())
        .ok_or_else(|| format!("像素尺寸对不上 {}×{}", size.0, size.1))?;
    buffer
        .save(path)
        .map_err(|e| format!("写 {} 失败：{e}", path.display()))
}
