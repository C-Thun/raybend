//! **镜头匹配冒烟**：读一张照片的 EXIF → 在 lensfun 库里自动匹配 → 解析出校正系数并打印。
//!
//! 用法：`cargo run -q -p raybend --example lens-probe -- <照片…>`
//!
//! 这个探针存在的理由：镜头匹配是**字符串模糊匹配**，光看单测过不过说明不了
//! 「真照片匹配到的是不是对的那一支」—— 真机上的判断要人来看（`AGENTS.md` §2.8）。

use std::path::Path;

use raybend::develop::lens::ManualLens;
use raybend::lens::{self, MatchInput, ShotParams};
use raybend::media::exif;

fn main() {
    let started = std::time::Instant::now();
    let db = lens::database();
    println!(
        "数据库：{} 支镜头 / {} 台机身（加载 + 解析 {:.1} ms）",
        db.map_or(0, |db| db.lenses.len()),
        db.map_or(0, |db| db.cameras.len()),
        started.elapsed().as_secs_f64() * 1000.0
    );

    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        println!("\n用法：cargo run -q -p raybend --example lens-probe -- <照片…>");
        return;
    }

    for path in paths {
        let path = Path::new(&path);
        println!("\n── {} ──", path.display());
        if !path.exists() {
            println!("文件不存在");
            continue;
        }
        let mut data = exif::read_file_for(path);
        if data.lens.is_none() && raybend::media::kind::kind_of_file(
            &path.file_name().unwrap_or_default().to_string_lossy(),
        ) == raybend::media::kind::MediaKind::Raw {
            data.lens = raybend::raw::worker::shared()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .lens_name(path)
                .ok()
                .flatten();
        }
        println!(
            "机身：{} {}   镜头：{}",
            data.camera_make.as_deref().unwrap_or("(无)"),
            data.camera_model.as_deref().unwrap_or("(无)"),
            data.lens.as_deref().unwrap_or("(无)")
        );
        println!(
            "焦距：{}   光圈：{}   尺寸：{}×{}",
            data.focal_mm.map_or("(无)".to_string(), |v| format!("{v:.1} mm")),
            data.f_number.map_or("(无)".to_string(), |v| format!("f/{v:.1}")),
            data.width.unwrap_or(0),
            data.height.unwrap_or(0)
        );

        let input = MatchInput {
            camera_make: data.camera_make.clone(),
            camera_model: data.camera_model.clone(),
            lens: data.lens.clone(),
        };
        let Some(profile) = lens::auto_match(&input) else {
            println!("匹配：**没有**（不猜 —— 界面上应显示「未匹配」）");
            continue;
        };
        println!(
            "匹配：{} {}（{}）",
            profile.maker,
            profile.model,
            if profile.rectilinear {
                "矩形"
            } else {
                "非矩形：几何校正本轮不做"
            }
        );
        let crop = lens::camera_crop(input.camera_make.as_deref(), input.camera_model.as_deref().unwrap_or(""))
            .unwrap_or(1.0);
        let shot = ShotParams {
            focal: data.focal_mm.unwrap_or(0.0) as f32,
            aperture: data.f_number.map(|v| v as f32),
            distance: None,
            camera_crop: crop,
            width: data.width.unwrap_or(6000).max(1) as u32,
            height: data.height.unwrap_or(4000).max(1) as u32,
        };
        println!("机身 crop：{crop:.2}");
        match lens::resolve(&profile.key, &shot, ManualLens::default()) {
            Some(resolved) => {
                let c = &resolved.correction;
                println!("  归一化 scale = {:.3e}，画幅角 = {:.4}", c.norm.scale, c.norm.half_diag);
                println!("  畸变：{:?}", c.distortion);
                println!("  色差：{:?}", c.tca);
                println!("  暗角：{:?}", c.vignetting);
                println!(
                    "  几何校正可用：{}；自动缩放 = {:.4}",
                    resolved.geometry,
                    raybend::develop::lens::auto_scale(c)
                );
            }
            None => println!("解析：**没有**（焦距缺失或该镜头没有标定）"),
        }
    }
}
