//! **sRGB 8bit 颜色** —— 目前只有一个用途：洞口底色（清屏色）。
//!
//! 为什么不直接在 `src-tauri` 里写 `wgpu::Color { .. }`：
//!
//! 1. `src-tauri` **不依赖 wgpu**（`AGENTS.md` §4：壳只管窗口与命令转发）；
//! 2. 更要紧的是「wgpu 的清屏值在 sRGB 目标上怎么解释」这件事**只该有一个答案** ——
//!    散在命令层写两遍，早晚会出现「这条路上偏亮一点」的颜色事故。
//!
//! # 约定（有实测背书，不是推的）
//!
//! wgpu 在 **sRGB 目标**上把清屏值当作**已经编码的显示值**直接写下去
//! （即 `r/255` 就是人眼看到的 `r`）。这条由
//! `crates/raybend/examples/editor-offscreen.rs` 用**外部给定的期望色**回读验证：
//! 「报 40/44/52，回读必须还是 40/44/52」（容差 1 级，留给重采样）。
//! 如果哪天换了后端/格式导致这条不再成立，那个例子会当场报红 ——
//! 而不要在这里凭记忆改公式。

/// 一个 sRGB 显示色（前端从 `getComputedStyle` 读到的那个值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Srgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Srgb8 {
    /// 深色主题的 `--surface-bar`（#2A2D33）—— 只是兜底，真实值由前端报。
    pub const DARK_SURFACE_BAR: Self = Self {
        r: 42,
        g: 45,
        b: 51,
    };

    /// 解析一段 CSS 颜色文本（前端把 `getComputedStyle` 的结果**原样**报上来）。
    ///
    /// 认这几种形态（都是浏览器实际会给出的形状）：
    ///
    /// | 形态 | 例子 | 备注 |
    /// | --- | --- | --- |
    /// | 十六进制 | `#2a2d33` / `#fff` / `#2a2d33cc` | 令牌层给的就是这个（`tokens.css`） |
    /// | 函数式 | `rgb(42, 45, 51)` `rgba(42, 45, 51, 1)` | `getComputedStyle().backgroundColor` |
    ///
    /// 三条规矩：
    ///
    /// * **不认缩写以外的东西**（`hsl`、颜色关键字直接返回 `None`）——
    ///   认不出来就报错/兜底，不让一个「猜出来的颜色」上屏；
    /// * **alpha 完全透明 → `None`**（`rgba(0, 0, 0, 0)` 的语义是「没设底色」，
    ///   拿它当洞口底色会直接透出桌面 —— 那是必须避开的观感事故）；
    /// * 半透明（0 < a < 1）按**不透明**用（洞口必须先有一层底，合成由 DWM 管；
    ///   我们不做半透明洞口那个复杂度）。
    #[must_use]
    pub fn parse_css(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some(hex) = text.strip_prefix('#') {
            return Self::parse_hex(hex);
        }
        let open = text.find('(')?;
        let close = text.rfind(')')?;
        if close <= open {
            return None;
        }
        let name = text[..open].trim().to_ascii_lowercase();
        if name != "rgb" && name != "rgba" {
            return None;
        }
        let mut parts = text[open + 1..close]
            .split(|ch: char| ch == ',' || ch == '/' || ch.is_ascii_whitespace())
            .filter(|part| !part.is_empty());
        let r = parts.next()?.parse::<f64>().ok()?;
        let g = parts.next()?.parse::<f64>().ok()?;
        let b = parts.next()?.parse::<f64>().ok()?;
        // alpha 可以不给（`rgb()` 的三参形态）；给了就必须是有限数
        let alpha = match parts.next() {
            Some(text) => text.parse::<f64>().ok()?,
            None => 1.0,
        };
        if !(r.is_finite() && g.is_finite() && b.is_finite() && alpha.is_finite()) {
            return None;
        }
        if alpha <= 0.0 {
            return None; // 「没有底色」不许当洞口底色
        }
        Some(Self {
            r: channel(r),
            g: channel(g),
            b: channel(b),
        })
    }

    /// `#rgb` / `#rrggbb` / `#rrggbbaa`（alpha 忽略：洞口底色一律不透明）。
    fn parse_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim();
        let value = |text: &str| u8::from_str_radix(text, 16).ok();
        match hex.len() {
            3 => {
                // `#abc` = `#aabbcc`（每一位重复一次）
                let expand = |ch: char| {
                    let digit = ch.to_digit(16)? as u8;
                    Some(digit * 16 + digit)
                };
                let mut chars = hex.chars();
                let r = expand(chars.next()?)?;
                let g = expand(chars.next()?)?;
                let b = expand(chars.next()?)?;
                Some(Self { r, g, b })
            }
            6 => Some(Self {
                r: value(&hex[0..2])?,
                g: value(&hex[2..4])?,
                b: value(&hex[4..6])?,
            }),
            8 => Some(Self {
                r: value(&hex[0..2])?,
                g: value(&hex[2..4])?,
                b: value(&hex[4..6])?,
            }),
            _ => None,
        }
    }

    /// → wgpu 的清屏色。
    ///
    /// ⚠️ **要做 sRGB → 线性的转换**，不能直接把 `r/255` 丢过去：
    ///
    /// wgpu 在 `*-srgb` 目标上把清屏值当作**线性值**，再编码成 sRGB 写进纹理。
    /// 这条不是推的，是 `examples/editor-offscreen.rs` 回读出来的：
    /// 直接传 `42/255` 时回读是 **110**（= `srgb_encode(42/255)`），
    /// 而不是 42 —— 那个例子把「报什么色就该回什么色」写成了外部给定的断言，
    /// 所以这条约定一旦变了（换后端/换格式）它会当场报红。
    ///
    /// 另外两条：
    /// * **alpha 恒为 1**：洞口底色透明就等于把桌面漏出来；
    /// * 采样纹理是 sRGB、写回也是 sRGB，所以**只有清屏这一步**需要线性值。
    #[must_use]
    pub fn to_clear_color(self) -> wgpu::Color {
        wgpu::Color {
            r: srgb_to_linear(f64::from(self.r) / 255.0),
            g: srgb_to_linear(f64::from(self.g) / 255.0),
            b: srgb_to_linear(f64::from(self.b) / 255.0),
            a: 1.0,
        }
    }
}

/// sRGB 编码值（0..1）→ 线性值（0..1）。
///
/// 就是 sRGB 传输函数的标准反变换（IEC 61966-2-1）：
/// 低段线性（≤ 0.04045），其余走 `((c + 0.055) / 1.055)^2.4`。
#[must_use]
fn srgb_to_linear(value: f64) -> f64 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// 线性值 → sRGB 编码值（测试里用来反推「回读到什么才算对」）。
#[cfg(test)]
#[must_use]
fn linear_to_srgb(value: f64) -> f64 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// 通道值 → 0..=255（越界夹住：CSS 允许百分比与超范围写法，我们只认数字形态）。
fn channel(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_in_three_forms() {
        let full = Srgb8::parse_css("#2a2d33").expect("六位十六进制");
        assert_eq!((full.r, full.g, full.b), (42, 45, 51));
        let short = Srgb8::parse_css(" #fff ").expect("三位缩写（两侧空白容忍）");
        assert_eq!((short.r, short.g, short.b), (255, 255, 255));
        let with_alpha = Srgb8::parse_css("#2a2d33cc").expect("八位（alpha 忽略）");
        assert_eq!((with_alpha.r, with_alpha.g, with_alpha.b), (42, 45, 51));
        let black = Srgb8::parse_css("#000").expect("黑色是合法的");
        assert_eq!((black.r, black.g, black.b), (0, 0, 0));
    }

    #[test]
    fn parses_computed_style_functions() {
        // 浏览器 `getComputedStyle().backgroundColor` 就是这个形状
        let rgb = Srgb8::parse_css("rgb(42, 45, 51)").expect("rgb()");
        assert_eq!((rgb.r, rgb.g, rgb.b), (42, 45, 51));
        let rgba = Srgb8::parse_css("rgba(42, 45, 51, 1)").expect("rgba()");
        assert_eq!((rgba.r, rgba.g, rgba.b), (42, 45, 51));
        let spaced = Srgb8::parse_css("rgb( 42 45 51 )").expect("空格分隔形态");
        assert_eq!((spaced.r, spaced.g, spaced.b), (42, 45, 51));
        let modern = Srgb8::parse_css("rgb(42 45 51 / 1)").expect("斜杠 alpha 形态");
        assert_eq!((modern.r, modern.g, modern.b), (42, 45, 51));
        // 半透明按不透明用（洞口必须先有一层底）
        let half = Srgb8::parse_css("rgba(42, 45, 51, 0.5)").expect("半透明合法");
        assert_eq!((half.r, half.g, half.b), (42, 45, 51));
        // 越界通道夹住（CSS 允许）
        let over = Srgb8::parse_css("rgb(300, -5, 51)").expect("越界合法");
        assert_eq!((over.r, over.g, over.b), (255, 0, 51));
        // 小数四舍五入
        let rounded = Srgb8::parse_css("rgb(42.6, 45.4, 50.5)").expect("小数合法");
        assert_eq!((rounded.r, rounded.g, rounded.b), (43, 45, 51));
    }

    #[test]
    fn rejects_what_it_cannot_vouch_for() {
        // 「没有底色」不许当洞口底色 —— 那会直接透出桌面
        assert!(Srgb8::parse_css("rgba(0, 0, 0, 0)").is_none());
        assert!(Srgb8::parse_css("transparent").is_none());
        // 认不出来的形态一律 None，不许猜
        for text in [
            "",
            "   ",
            "hsl(210, 10%, 18%)",
            "red",
            "rgb(42, 45)",
            "rgb(a, b, c)",
            "#12",
            "#12345",
            "#gggggg",
            "rgb(42, 45, 51",
            "42, 45, 51",
            "rgb(42, 45, NaN)",
        ] {
            assert!(Srgb8::parse_css(text).is_none(), "{text:?} 不该被认出");
        }
    }

    #[test]
    fn clears_are_opaque_and_linearised() {
        let color = Srgb8 {
            r: 42,
            g: 45,
            b: 51,
        }
        .to_clear_color();
        // 传下去的是**线性值**：回读时硬件会再编码回 sRGB，正好得到 42/45/51
        for (actual, expected) in [(color.r, 42.0), (color.g, 45.0), (color.b, 51.0)] {
            let back = linear_to_srgb(actual) * 255.0;
            assert!(
                (back - expected).abs() < 0.5,
                "线性化之后回读应当回到原值：{back} vs {expected}"
            );
        }
        assert!((color.a - 1.0).abs() < 1e-12, "洞口底色必须不透明");
    }

    #[test]
    fn linearisation_matches_the_srgb_transfer_function() {
        // 端点与中段各钉一个值（外部给定的常数，不是自己算出来的）
        assert!((srgb_to_linear(0.0) - 0.0).abs() < 1e-12);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-9);
        assert!((srgb_to_linear(0.5) - 0.21404114).abs() < 1e-6, "中灰 128 的线性值是 0.214");
        assert!((srgb_to_linear(0.04) - 0.04 / 12.92).abs() < 1e-12, "低段走线性那一段");
        // 单调（颜色转换搞反的常见症状是某些区段反向）
        let mut previous = -1.0;
        for step in 0..=32 {
            let value = f64::from(step) / 32.0;
            let linear = srgb_to_linear(value);
            assert!(linear > previous, "必须单调递增：{value} → {linear}");
            previous = linear;
        }
    }

    #[test]
    fn extremes_hit_exactly_zero_and_one() {
        let black = Srgb8 { r: 0, g: 0, b: 0 }.to_clear_color();
        assert_eq!((black.r, black.g, black.b), (0.0, 0.0, 0.0));
        let white = Srgb8 {
            r: 255,
            g: 255,
            b: 255,
        }
        .to_clear_color();
        assert_eq!((white.r, white.g, white.b), (1.0, 1.0, 1.0));
        // 通道不许串（红蓝互换这类事故在颜色代码里很常见）
        let red = Srgb8 { r: 255, g: 0, b: 0 }.to_clear_color();
        assert_eq!((red.r, red.g, red.b), (1.0, 0.0, 0.0));
    }
}
