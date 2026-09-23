//! spike 用的**合成测试图**（`PLAN.md` A.2 的性能基线：6000×4000）。
//!
//! 为什么要专门生成一张图而不是拿张照片：spike 要回答的是**几件必须肉眼可判**的事
//! ——「1 图像像素是不是 1 个物理像素」「转屏之后方位对不对」「缩放时有没有莫尔纹」
//! 「坐标同步有没有偏」。拿真实照片看这些，每次都要靠记忆比；合成图把答案**画在图里**：
//!
//! | 图案 | 用来判什么 |
//! | --- | --- |
//! | 四角**方位标记**（颜色与形状各不相同） | 方向/镜像有没有搞错，旋转对不对 |
//! | 上边缘的**刻度尺**（每 100 短、每 500 长并标块） | 1:1 档位下能不能看清 1px |
//! | 边缘的 **1px 棋盘格** | 缩小时的莫尔纹与重采样质量（与 A.2 的「无闪烁」相关） |
//! | 中心的**十字** | 坐标同步：十字心必须落在图像正中心 |
//! | 平滑**渐变** | 8bit 阶梯（色带）与 sRGB 处理是否一致 |
//!
//! 生成是纯 CPU、确定性的（同一个 `seed` 出同一张图）—— 这样报告里的数字可以复现。
//!
//! 图本身的类型是 [`super::image::RenderImage`]（M3-W2 起与真实照片共用同一个类型）。

use super::image::RenderImage;

/// 生成测试图。`width`/`height` 为 0 时返回一张 1×1（避免 0 尺寸纹理导致 wgpu 报错）。
pub fn make_test_image(width: u32, height: u32) -> RenderImage {
    let width = width.max(1);
    let height = height.max(1);
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;

    for y in 0..height {
        for x in 0..width {
            let fx = x as f32;
            let fy = y as f32;
            let index = ((y as usize) * (width as usize) + x as usize) * 4;

            // ① 底：对角渐变（左上是暗灰、右下是亮灰），用来找色带
            let t = ((fx / width as f32) + (fy / height as f32)) / 2.0;
            let base = (24.0 + t * 200.0) as u8;
            let mut rgb = [base, base, base];

            // ② 左右两侧的 1px 棋盘格（各 64px 宽）：缩小时的莫尔纹看这里
            let checker_zone = 64;
            if x < checker_zone || x >= width.saturating_sub(checker_zone) {
                let tone = if (x + y) % 2 == 0 { 255u8 } else { 0u8 };
                rgb = [tone, tone, tone];
            }

            // ③ 上边缘刻度尺：每 100px 一根短标，每 500px 一根长标（下探 24px）
            if y < 24 {
                if x % 500 == 0 {
                    rgb = [255, 255, 255];
                } else if y < 12 && x % 100 == 0 {
                    rgb = [180, 180, 180];
                }
            }

            // ④ 四角方位标记：**刻意各不相同**（颜色 + 形状），一眼看出有没有镜像/转错
            //    左上=红、右上=绿、右下=蓝、左下=黄；每块 120×120，只画在各自那一角
            let mark = 120u32;
            let in_tl = x < mark && y < mark;
            let in_tr = x >= width.saturating_sub(mark) && y < mark;
            let in_br = x >= width.saturating_sub(mark) && y >= height.saturating_sub(mark);
            let in_bl = x < mark && y >= height.saturating_sub(mark);
            if in_tl || in_tr || in_br || in_bl {
                // 角内坐标：**一律「距自己那个角的距离」**（左上取 x/y，右上取 宽度−1−x 等）。
                // 早先按「距标记框左边界」算，右上的方块就落到了框的左端 —— 角上一个像素都没盖住。
                let lx = x.min(mark - 1);
                let ly = y.min(mark - 1);
                let rx = (width - 1 - x).min(mark - 1);
                let by = (height - 1 - y).min(mark - 1);
                let block = 80u32;
                let arm = 16u32;
                let keep = if in_tl {
                    // 左上：方块 + 向右的臂
                    (lx < block && ly < block) || (ly < arm && lx < mark)
                } else if in_tr {
                    // 右上：方块 + 向下的臂
                    (rx < block && ly < block) || (rx < arm && ly < mark)
                } else if in_br {
                    // 右下：方块 + 向左的臂
                    (rx < block && by < block) || (by < arm && rx < mark)
                } else {
                    // 左下：方块 + 向上的臂
                    (lx < block && by < block) || (lx < arm && by < mark)
                };
                if keep {
                    rgb = if in_tl {
                        [220, 40, 40]
                    } else if in_tr {
                        [40, 200, 60]
                    } else if in_br {
                        [60, 90, 230]
                    } else {
                        [235, 200, 40]
                    };
                }
            }

            // ⑤ 中心十字（长 200px、宽 3px）：坐标同步的锚点，十字心必须正好是图心
            let cross_arm = 100.0f32;
            let on_v = (fx - cx).abs() <= 1.5 && (fy - cy).abs() <= cross_arm;
            let on_h = (fy - cy).abs() <= 1.5 && (fx - cx).abs() <= cross_arm;
            if on_v || on_h {
                rgb = [255, 0, 255];
            }

            // ⑥ 图像正中心 8×8 的纯白块：1:1 档位下「一个图像像素就是一个屏幕像素」的判据
            if (fx - cx).abs() < 4.0 && (fy - cy).abs() < 4.0 {
                rgb = [255, 255, 255];
            }

            pixels[index] = rgb[0];
            pixels[index + 1] = rgb[1];
            pixels[index + 2] = rgb[2];
            pixels[index + 3] = 255; // 图像本身不透明；透明只发生在「洞口之外」
        }
    }

    RenderImage {
        width,
        height,
        pixels,
    }
}

/// 图心（浮点）—— 坐标检查用。
pub fn image_center(image: &RenderImage) -> (f32, f32) {
    (image.width as f32 / 2.0, image.height as f32 / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 取一个像素的 **RGB**（alpha 单独验，见 `every_pixel_is_opaque`）。
    fn px(image: &RenderImage, x: u32, y: u32) -> [u8; 3] {
        let i = ((y as usize) * (image.width as usize) + x as usize) * 4;
        [image.pixels[i], image.pixels[i + 1], image.pixels[i + 2]]
    }

    #[test]
    fn size_and_buffer_match() {
        let image = make_test_image(6000, 4000);
        assert_eq!(image.width, 6000);
        assert_eq!(image.height, 4000);
        assert_eq!(image.byte_len(), 6000 * 4000 * 4);
    }

    #[test]
    fn zero_size_does_not_explode() {
        // wgpu 不接受 0 尺寸纹理，所以这里必须兜住
        let image = make_test_image(0, 0);
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(image.byte_len(), 4);
    }

    #[test]
    fn every_pixel_is_opaque() {
        // 图像自己必须不透明：透明只该发生在洞口之外，否则「挖洞」没法排查
        let image = make_test_image(64, 48);
        for chunk in image.pixels.as_chunks::<4>().0 {
            assert_eq!(chunk[3], 255);
        }
    }

    #[test]
    fn center_is_white_and_marks_the_image_middle() {
        let image = make_test_image(1000, 800);
        assert_eq!(px(&image, 500, 400), [255, 255, 255], "正中心必须是白块");
        // 白块只有 8×8：往外 10px 就不是纯白了
        assert_ne!(px(&image, 512, 400), [255, 255, 255]);
    }

    #[test]
    fn crosshair_arms_are_magenta() {
        let image = make_test_image(600, 400);
        let (cx, cy) = image_center(&image);
        // 十字横臂（偏离中心 60px）
        let x = (cx as u32) + 60;
        assert_eq!(px(&image, x, cy as u32), [255, 0, 255]);
        // 十字竖臂
        let y = (cy as u32) + 60;
        assert_eq!(px(&image, cx as u32, y), [255, 0, 255]);
    }

    #[test]
    fn orientation_marks_differ_at_four_corners() {
        let image = make_test_image(1000, 800);
        let tl = px(&image, 5, 5);
        let tr = px(&image, 995, 5);
        let br = px(&image, 995, 795);
        let bl = px(&image, 5, 795);
        assert_eq!(tl, [220, 40, 40], "左上应当是红的");
        assert_eq!(tr, [40, 200, 60], "右上应当是绿的");
        assert_eq!(br, [60, 90, 230], "右下应当是蓝的");
        assert_eq!(bl, [235, 200, 40], "左下应当是黄的");
    }

    #[test]
    fn ruler_ticks_are_on_the_top_edge() {
        let image = make_test_image(1000, 800);
        // x=300 起（避开四角标记——它们盖住前 120px，会把刻度色盖成标记色）
        assert_eq!(px(&image, 500, 0), [255, 255, 255], "每 500px 一根白长标");
        assert_eq!(px(&image, 300, 0), [180, 180, 180], "每 100px 一根灰短标");
        assert_ne!(px(&image, 301, 0), [255, 255, 255]);
        // 刻度只在顶上 24px 之内
        assert_ne!(px(&image, 500, 30), [255, 255, 255]);
    }

    #[test]
    fn checker_zones_are_alternating_black_and_white() {
        // 取 y=200：要避开四角标记（前 120px）与顶部刻度尺（前 24px）
        let image = make_test_image(1000, 800);
        assert_eq!(px(&image, 0, 200)[0], 255, "(0,200) 是白");
        assert_eq!(px(&image, 1, 200)[0], 0, "(1,200) 是黑");
        assert_eq!(px(&image, 0, 201)[0], 0, "(0,201) 是黑");
        assert_eq!(px(&image, 1, 201)[0], 255, "(1,201) 是白");
    }

    #[test]
    fn corner_marks_cover_their_corners_with_distinct_shapes() {
        let image = make_test_image(1000, 800);
        // 四个角的最角那个像素必须是各自亮色 —— 将来改图案时别把这四个遮住
        assert_ne!(px(&image, 0, 0), px(&image, 0, 799));
        assert_ne!(px(&image, 999, 0), px(&image, 999, 799));
        // 臂的朝向各不相同：左上向右伸、右上向下伸、右下向左伸、左下向上伸
        assert_eq!(px(&image, 100, 8), [220, 40, 40], "左上的臂向右伸");
        assert_eq!(px(&image, 992, 100), [40, 200, 60], "右上的臂向下伸");
        assert_eq!(px(&image, 899, 792), [60, 90, 230], "右下的臂向左伸");
        assert_eq!(px(&image, 8, 700), [235, 200, 40], "左下的臂向上伸");
    }

    #[test]
    fn generation_is_deterministic() {
        let a = make_test_image(320, 240);
        let b = make_test_image(320, 240);
        assert_eq!(a.pixels, b.pixels);
    }

    #[test]
    fn baseline_size_is_the_planned_6000x4000() {
        // A.2 的性能基线就是 6000×4000（6000×4000×4 ≈ 96MB）——
        // 这条测试同时把「内存账」钉在代码里
        let image = make_test_image(6000, 4000);
        assert_eq!(image.byte_len(), 96_000_000);
    }
}
