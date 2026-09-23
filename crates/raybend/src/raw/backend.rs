//! RAW 后端的公共类型与接口。
//!
//! 这里**不出现任何 rawler 类型**（`AGENTS.md` §6.3 的第 1 条）。
//! 换后端时只改 [`crate::raw::rawler_backend`]，上层不受影响。

use std::path::PathBuf;

/// 像素从哪来 —— 决定了耗时与画质，UI 与缓存诊断都用得上。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PixelSource {
    /// 相机写在 RAW 里的 JPEG 预览（快路径）。
    EmbeddedPreview,
    /// 真实解码（黑电平 / 白平衡 / 色彩矩阵 / 伽马）。
    Decoded,
}

impl PixelSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmbeddedPreview => "embedded-preview",
            Self::Decoded => "decoded",
        }
    }

    /// 是不是「已经把 EXIF 方向烤进像素里了」。
    ///
    /// 内嵌预览**不好说**（相机可能写的就是摆正后的图，也可能不是），
    /// 所以两条路径都返回 `false`，方向一律交给调用方按 EXIF 摆正 —— 幂等地做一次，
    /// 比「猜相机有没有转过」可靠。
    #[must_use]
    pub const fn applies_orientation(self) -> bool {
        false
    }
}

/// 一张解好的 RAW 图：**8 位 RGB，行优先紧密排列**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImage8 {
    pub width: u32,
    pub height: u32,
    /// `width * height * 3` 个字节。
    pub rgb: Vec<u8>,
    pub source: PixelSource,
    /// EXIF 方向（1–8）；`None` = 文件里没有，按 1 处理。
    pub orientation: Option<u16>,
}

impl RawImage8 {
    /// 自查：长度与声明的尺寸必须对得上（进程边界上来的数据要防一手）。
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.rgb.len() == (self.width as usize) * (self.height as usize) * 3
    }
}

/// 一张解好的 RAW 图：**线性 sRGB，16 位**（M3-W3 的显影管线输入）。
///
/// # 为什么是线性 sRGB、不是「相机空间」
///
/// rawler 的显影链在 `Calibrate` 步就把相机空间映射到了 **sRGB 原色的线性光**
/// （`rgb2cam = normalize(xyz2cam · SRGB_TO_XYZ_D65)` 的伪逆）—— 所以到 `Calibrate`
/// 为止、**去掉 `SRgb` 伽马步**，拿到的就是「线性 sRGB」。
/// 这正是 `FUTURE.md` D1 要的 scene-referred 输入：调性在它上面做，不会在 8bit 显示空间里出色带。
///
/// # 精度
///
/// 1.0 映射到 `65535`。u16 的相对精度（1.5e-5）远好于 8bit 显示（1/255），
/// 而内存只有 f32 的一半 —— 24MP 三通道 144MB（f32 要 288MB）。
#[derive(Debug, Clone, PartialEq)]
pub struct RawImage16 {
    pub width: u32,
    pub height: u32,
    /// `width * height * 3` 个值（线性 sRGB，0..=65535 ↔ 0.0..=1.0）。
    pub rgb: Vec<u16>,
    pub source: PixelSource,
    /// EXIF 方向（1–8）；`None` = 文件里没有，按 1 处理。
    pub orientation: Option<u16>,
    /// **拍摄时的色温估计**（K）：从相机的白平衡系数 + 色彩矩阵反算出来的。
    ///
    /// 它是编辑器里色温拉杆的**基线**（`AGENTS.md` §11.5：载入照片时标尺要移到这个位置）。
    /// `None` = 这台相机/这个文件里算不出来，调用方退回默认值。
    pub as_shot_temperature: Option<f32>,
}

impl RawImage16 {
    /// 自查：长度与声明的尺寸必须对得上。
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.rgb.len() == (self.width as usize) * (self.height as usize) * 3
    }
}

/// 一次解码请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeRequest {
    pub path: PathBuf,
    /// 长边上限；`None` = 原始尺寸。
    ///
    /// **在 worker 里缩放**：一张 6000×4000 的 RGB 是 72MB，跨进程搬它没有意义。
    pub max_edge: Option<u32>,
    /// 允许走内嵌预览快路径（够大就用它）。
    pub allow_preview: bool,
}

impl DecodeRequest {
    /// 缩略图请求（网格 / 胶片带 / 看图都用它）。
    #[must_use]
    pub fn thumb(path: impl Into<PathBuf>, max_edge: u32) -> Self {
        Self {
            path: path.into(),
            max_edge: Some(max_edge),
            allow_preview: true,
        }
    }

    /// 原尺寸完整解码（1:1 与将来的显影）。
    #[must_use]
    pub fn full(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            max_edge: None,
            allow_preview: false,
        }
    }

    /// 改「允不允许内嵌预览快路径」（两个构造函数之外的第三个开关）。
    ///
    /// 存在的理由：编辑器要的两档（内嵌预览 / 完整解码）与缩略图的请求形状不同 ——
    /// 前者可能「限了长边但仍要完整解码」，用构造函数拼不出来。
    #[must_use]
    pub fn with_preview(mut self, allow: bool) -> Self {
        self.allow_preview = allow;
        self
    }
}

/// 后端自身的错误。**不进 `crate::Error`** —— 由 [`crate::raw::worker`] 翻译成
/// 带进程语义的错误（崩溃 / 超时 / 协议错误都在那里）。
#[derive(Debug, thiserror::Error)]
pub enum RawError {
    /// 读不到文件（不存在、权限、被占用）。
    #[error("读不到这个文件：{0}")]
    Io(String),
    /// 文件头不像任何已知的 RAW 容器。
    #[error("文件头不像 RAW：{0}")]
    NotRaw(String),
    /// 相机型号或文件格式暂不支持（rawler 的 `Unsupported`）。
    #[error("这台相机或这个格式暂不支持：{0}")]
    Unsupported(String),
    /// 文件损坏 / 解码中途失败。
    #[error("解码失败（文件可能损坏）：{0}")]
    Decode(String),
    /// 解码成功但结果不可用（尺寸为 0、像素长度对不上等）。
    #[error("解码结果不可用：{0}")]
    Empty(String),
}

pub type RawResult<T> = std::result::Result<T, RawError>;

/// 可插拔的 RAW 后端。
///
/// 实现者**只应在 worker 进程内被调用**（`AGENTS.md` §6.3 第 2 条）。
pub trait RawBackend: Send + Sync {
    /// 后端名（日志与诊断）。
    fn name(&self) -> &'static str;

    /// 解码一张 RAW（8bit sRGB，给网格/看图/缩略图）。
    fn decode(&self, req: &DecodeRequest) -> RawResult<RawImage8>;

    /// 解码一张 RAW 成**线性 16 位**（给显影管线）。
    ///
    /// 默认实现报「不支持」—— 换后端时（`FUTURE.md` §B）若新后端没有线性输出，
    /// 失败是显式的，而不是静默退回 8bit（那会让「scene-referred」变成一句空话）。
    ///
    /// # Errors
    /// 见 [`RawError`]。
    fn decode_linear(&self, req: &DecodeRequest) -> RawResult<RawImage16> {
        let _ = req;
        Err(RawError::Unsupported(format!(
            "后端 {} 不支持线性解码",
            self.name()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_request_defaults_to_preview_path() {
        let req = DecodeRequest::thumb("/x/IMG.RW2", 768);
        assert_eq!(req.max_edge, Some(768));
        assert!(req.allow_preview, "缩略图必须允许快路径");
    }

    #[test]
    fn preview_flag_can_be_switched_on_a_limited_request() {
        // 编辑器那一档：**限了长边但不要内嵌预览**（要真解码的像素）
        let req = DecodeRequest::thumb("/x/IMG.RW2", 2048).with_preview(false);
        assert_eq!(req.max_edge, Some(2048));
        assert!(!req.allow_preview);
        let back = DecodeRequest::full("/x/IMG.RW2").with_preview(true);
        assert_eq!(back.max_edge, None);
        assert!(back.allow_preview);
    }

    #[test]
    fn full_request_forbids_preview() {
        let req = DecodeRequest::full("/x/IMG.RW2");
        assert_eq!(req.max_edge, None);
        assert!(!req.allow_preview, "1:1 必须真解码");
    }

    #[test]
    fn consistency_check_catches_truncated_pixels() {
        let ok = RawImage8 {
            width: 2,
            height: 2,
            rgb: vec![0; 12],
            source: PixelSource::Decoded,
            orientation: None,
        };
        assert!(ok.is_consistent());

        let short = RawImage8 {
            rgb: vec![0; 11],
            ..ok.clone()
        };
        assert!(!short.is_consistent(), "少一个字节都不行");

        let zero = RawImage8 {
            width: 0,
            rgb: Vec::new(),
            ..ok
        };
        assert!(!zero.is_consistent(), "0 宽不是合法结果");
    }

    #[test]
    fn source_strings_are_stable() {
        // 这两个字符串会进日志与诊断报告，改名会让旧记录对不上
        assert_eq!(PixelSource::EmbeddedPreview.as_str(), "embedded-preview");
        assert_eq!(PixelSource::Decoded.as_str(), "decoded");
        assert!(!PixelSource::Decoded.applies_orientation());
    }
}
