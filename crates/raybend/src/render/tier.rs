//! **取图档位的迟滞**（编辑器视口用）。
//!
//! 视口里同一张照片有两个来源，代价差 1–2 个数量级：
//!
//! | 档 | 来源 | 典型耗时 | 用途 |
//! | --- | --- | --- | --- |
//! | [`ImageTier::Preview`] | RAW 内嵌预览 / 位图缩到屏幕档 | 毫秒级 | 进视口、适合窗口、缩小看 |
//! | [`ImageTier::Full`] | 完整解码（RAW 走传感器数据） | 秒级（RAW） | `1:1` 看真实像素 |
//!
//! 「什么时候该要哪一档」如果直接写成 `zoom >= 1.0`，用户把滚轮停在 1.0 附近抖动时
//! 会**反复触发完整解码**（RAW 上一次几百毫秒到几秒），画面一卡一卡 —— 所以这里要迟滞：
//!
//! ```text
//!   升档（要全尺寸）：zoom ≥ FULL_ENTER_ZOOM = 1.0
//!   降档（回预览）  ：zoom <  FULL_EXIT_ZOOM  = 0.5
//! ```
//!
//! 纯函数，边界都在测试里钉住（`PLAN.md` §M3-W2 的 DoD 第 2 条要「两条路都在」，
//! 而「什么时候走哪条」是这件事里唯一需要判定的部分）。

/// 图像的取图档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageTier {
    /// 屏幕档（长边 ≤1920；RAW 优先用内嵌预览）。
    Preview,
    /// 原尺寸（1:1 看像素）。
    Full,
}

impl ImageTier {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Full => "full",
        }
    }
}

/// 升到全尺寸的阈值（1 CSS 像素 = 1 图像像素起就该看真实像素了）。
pub const FULL_ENTER_ZOOM: f32 = 1.0;

/// 退回预览档的阈值（**必须小于**升档阈值，否则就是抖动放大器）。
pub const FULL_EXIT_ZOOM: f32 = 0.5;

/// 按当前缩放与当前档位，算出**这一刻应该用哪一档**。
///
/// * `zoom` 不是有限数（理论上进不来，进得来就是上游错了）→ **保持现状**，不擅自换档；
/// * 已经在 `Full` 时只有掉到 [`FULL_EXIT_ZOOM`] 以下才回 `Preview`（迟滞）。
#[must_use]
pub fn tier_for(zoom: f32, current: ImageTier) -> ImageTier {
    if !zoom.is_finite() {
        return current;
    }
    match current {
        ImageTier::Preview => {
            if zoom >= FULL_ENTER_ZOOM {
                ImageTier::Full
            } else {
                ImageTier::Preview
            }
        }
        ImageTier::Full => {
            if zoom < FULL_EXIT_ZOOM {
                ImageTier::Preview
            } else {
                ImageTier::Full
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_escalates_only_at_one_to_one() {
        assert_eq!(tier_for(0.99, ImageTier::Preview), ImageTier::Preview);
        assert_eq!(
            tier_for(FULL_ENTER_ZOOM, ImageTier::Preview),
            ImageTier::Full,
            "正好 1:1 就该要真实像素"
        );
        assert_eq!(tier_for(4.0, ImageTier::Preview), ImageTier::Full);
    }

    #[test]
    fn full_falls_back_only_below_the_exit_threshold() {
        // 1.0 上下抖动时**不许**换档（迟滞的全部意义）
        for zoom in [1.0, 0.99, 0.75, FULL_EXIT_ZOOM] {
            assert_eq!(
                tier_for(zoom, ImageTier::Full),
                ImageTier::Full,
                "zoom={zoom} 时不该退回预览（会反复触发完整解码）"
            );
        }
        assert_eq!(tier_for(0.499, ImageTier::Full), ImageTier::Preview);
        assert_eq!(tier_for(0.1, ImageTier::Full), ImageTier::Preview);
    }

    #[test]
    fn hysteresis_band_never_oscillates() {
        // 在 [0.5, 1.0) 这段里来回走：每次都要么不变、要么是一种方向 —— 不可能来回翻
        let mut tier = ImageTier::Preview;
        let mut changes = 0;
        for _ in 0..10 {
            for zoom in [0.5f32, 0.6, 0.9, 0.99, 0.7, 0.55] {
                let next = tier_for(zoom, tier);
                if next != tier {
                    changes += 1;
                }
                tier = next;
            }
        }
        assert_eq!(tier, ImageTier::Preview, "整段都在 1.0 以下，应当仍是预览档");
        assert_eq!(changes, 0, "这段区间里一次档位都不该换");
    }

    #[test]
    fn nonsense_zoom_keeps_the_current_tier() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(tier_for(bad, ImageTier::Preview), ImageTier::Preview);
            assert_eq!(tier_for(bad, ImageTier::Full), ImageTier::Full);
        }
    }

    #[test]
    fn thresholds_are_ordered_and_named_as_the_wire_format() {
        // 迟滞成立的前提：退出阈值必须**小于**进入阈值（编译期就钉住 —— 改错了构建不过）
        const { assert!(FULL_EXIT_ZOOM < FULL_ENTER_ZOOM, "退出阈值必须小于进入阈值，否则迟滞不成立") };
        assert_eq!(ImageTier::Preview.as_str(), "preview");
        assert_eq!(ImageTier::Full.as_str(), "full");
    }
}
