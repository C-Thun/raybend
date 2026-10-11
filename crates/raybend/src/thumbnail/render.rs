//! 缩略图渲染：解码 → 按 EXIF 朝向摆正 → 缩放 → 编码 JPEG。
//!
//! # 尺度（`AGENTS.md` §6.5、`specs/thumbnail-tiers-and-import-memory.md`）
//!
//! **缩图只有两档**（崔总 2026-10-11 定：192 那一档取消 —— 384 直接缩着显示就行，
//! 没必要为胶片带多算一遍缩图）：
//!
//! * [`SizeClass::Grid`] —— 缩略图，长边 384：网格 tiles、**所有**胶片带、定稿卡片都用它；
//! * [`SizeClass::Screen`] —— 载入占位 / 总览（长边 1920）；
//! * [`SizeClass::Full`] —— 真身（不缩放），看图终态（`specs/view-true-image.md`）。
//!
//! # 渲染签名
//!
//! [`render_sig`] 把「编码格式 + 质量 + 管线版本」编成一个串，进缓存键。
//! **改了算法就改版本号** —— 旧缓存会自动变成孤儿被 GC 收走，不用写数据迁移。
//!
//! # RAW 与「只有 RAW」
//!
//! RAW **不在本进程里解码** —— 走 [`crate::raw`] 的 worker 进程（内嵌预览优先，
//! 完整解码留给 1:1；见 `AGENTS.md` §6.3）。本模块负责的是「拿到 8 位 RGB 之后」
//! 的那一段：方向、夹取、缩放、编码。
//!
//! 真解不开时（相机不支持、文件损坏）才用 [`placeholder`] 生成的占位图 ——
//! 占位图是程序画的，**不依赖任何字体文件**。

use std::path::Path;

use image::{DynamicImage, ExtendedColorType, ImageFormat, Rgb, RgbImage};

use image::ImageEncoder;

use crate::develop::pipeline::LinearImage;
use crate::store::develop::DevelopStack;

use crate::error::{Error, Result};
use crate::media::kind::{MediaKind, kind_of_file};

/// 缩略图长边（网格 tiles、所有胶片带、定稿卡片共用这一档）。
pub const GRID_LONG_EDGE: u32 = 384;
/// 屏幕（看图用；`AGENTS.md` §6.5 的 SCREEN 档 —— 视口级别，比网格大一档）。
///
/// 1920 是个折中：绝大多数显示器 1:1 看已经够清楚，解码与缓存代价又远小于原图。
/// 真正的 1:1 原图（FULL 档）属于后续里程碑，这一版看图先用它。
pub const SCREEN_LONG_EDGE: u32 = 1920;
/// **AVIF 的全局口径**（人类 2026-09-24 定）：质量 90、次级采样 4:4:4。
///
/// 4:4:4 不是「随手选的参数」：ravif 只有走 **RGB8**（`encode_rgb`）才是 4:4:4，
/// 走 RGBA8 是 4:2:0 —— 所以缩略图与缓存图**一律丢 alpha** 再编码
/// （照片本来也没有 alpha；见 [`encode_avif`]）。
pub const AVIF_QUALITY: u8 = 90;

/// AVIF 编码速度（1 最慢 / 10 最快）。**取 10**，理由是一组实测（22 线程，合成图，
/// `examples/avif-probe.rs` 跑出来的；真实照片会更低）：
///
/// | 尺寸 | 速度 10 | 速度 8 | 同尺寸 JPEG q82 |
/// | --- | --- | --- | --- |
/// | 384×288 | 62ms / 5KB | 170ms / 4KB | 5ms / 12KB |
/// | 1920×1280 | 539ms / 115KB | 1.9s / 82KB | 100ms / 270KB |
/// | 2560×1707 | 904ms / 208KB | 4.1s / 146KB | 193ms / 481KB |
///
/// 慢档位（8）只换来 20–30% 的体积，却要 **3 倍**时间 —— 对一个「随时可能重写」的缓存
/// 不划算。**没开线程时**（`image` 的 `rayon` feature）同样的速度 10 要 193ms/1920 档，
/// 慢 8 倍以上，所以线程是必须的。
pub const AVIF_SPEED: u8 = 10;

/// **缓存图的编码格式 —— 由场景决定**（崔总 2026-10-11 定；`AGENTS.md` §2.24）。
///
/// * [`EncodeFormat::Avif`]：**库内**（浏览 / 编辑 / 导出）。库里有 catalog + 反复访问，
///   为画质与磁盘占用付编解码代价是值的（AVIF q90 / 4:4:4）。
/// * [`EncodeFormat::Jpeg`]：**库外**（import 的来源目录）。那里只求「最快出现」——
///   来源是相机卡 / 待入老库 / 临时盘，导入完就不会再回来，画质与体积都可以让步。
///
/// 实测（同一张 384px 图，Windows release，真照片）：AVIF q90 要 **118–166 ms**，
/// JPEG q82 只要 **3–5 ms**（体积 25 KB → 27 KB）——给 import 做 AVIF 是纯负优化。
/// 依据 `implementations/2026-10-11_import-tiles-diagnosis.md`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncodeFormat {
    #[default]
    Avif,
    Jpeg,
}

/// **库外（import）小图的质量**：q82 —— import 第一版（2026-09-16）就是这个值，
/// 也是实测「3–5 ms/张」那一档（`AGENTS.md` §2.24）。
pub const JPEG_THUMB_QUALITY: u8 = 82;

/// **AVIF 编码**（库内缓存图的唯一出口；库外走 [`EncodeFormat::Jpeg`]）。
///
/// # Errors
/// 编码器内部错误（尺寸为 0、数据长度对不上…）。
pub fn encode_avif(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    encode_avif_with_metadata(rgb, width, height, AVIF_QUALITY, Vec::new())
}
pub fn encode_avif_with_metadata(rgb: &[u8], width:u32,height:u32,quality:u8,exif:Vec<u8>) -> Result<Vec<u8>> {
    if width == 0 || height == 0 {
        return Err(Error::Unsupported("AVIF 编码：尺寸为 0".to_string()));
    }
    let expected = (width as usize) * (height as usize) * 3;
    if rgb.len() != expected {
        return Err(Error::Unsupported(format!(
            "AVIF 编码：像素长度对不上（期望 {expected}，收到 {}）",
            rgb.len()
        )));
    }
    let mut data = Vec::new();
    let mut encoder = image::codecs::avif::AvifEncoder::new_with_speed_quality(
        &mut data,
        AVIF_SPEED,
        quality,
    ).with_num_threads(Some(1));
    if !exif.is_empty() { encoder.set_exif_metadata(exif).map_err(|e| Error::Unsupported(e.to_string()))?; }
    // RGB8（不是 RGBA8）⇒ ravif 走 4:4:4 色度采样（人类 2026-09-24 的口径）
    encoder
        .write_image(rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| Error::Unsupported(format!("AVIF 编码失败：{e}")))?;
    let profile=crate::color::icc::srgb_icc().map_err(|e|Error::Unsupported(e.to_string()))?;
    crate::media::isobmff::attach_icc(&data,profile.bytes())
}
/// 渲染管线版本：**算法一改就 +1**（缓存靠它自动失效）。
///
/// # v3（2026-09-17）：RAW 从占位图改成真解码
///
/// RAW 支持落地时**漏了抬版本号**，后果很隐蔽：早先导入的 RAW 已经把占位图按 `v2`
/// 缓存住了，改完算法之后网格**仍然拿回那张占位块**（缓存命中，永远不会重渲染）——
/// 现象就是「RAW 在预览列表里没有画面」，而且没有任何报错。抬到 v3 之后旧签名变成孤儿，
/// 由缓存 GC 收走（`cache.rs` 的 `DELETE FROM thumbs WHERE render_sig NOT IN (…)`），
/// 下次打开自然重新出图。
///
/// **纪律**：只要改动了「同一份输入会渲染出不同像素」的东西（解码路径、方向处理、编码参数、
/// 叠加内容），就必须 +1 —— 否则用户看到的是**旧算法**的结果，而程序一切正常。
///
/// **第二次踩同一个坑**（2026-09-17，v3 → v4）：RAW 的**方向处理**（`render_raw_file` 里
/// 从文件头读 orientation 的那段）是在抬到 v3 **之后半小时**才进去的 ——
/// 于是 v3 签名对「转向之前渲染的纵拍 RAW」依然命中，人类看到的是
/// 「**tile 的框是纵的、框里的图是横的**」（框的纵横比走的是新元数据，而缩略图是旧缓存）。
/// 实测证据：拿同一张 `P1000023.RW2` 跑 `thumb-probe`，现管线输出的是 **288×384（纵）** ✓，
/// 说明代码没问题、就是缓存没作废。
///
/// **第三次（v4 → v5，2026-09-18）**：改「方向优先级」那次（worker 的值不该压过文件头）
/// 我**没有**一起抬版本 —— 于是 11:44–12:57 之间渲染出的 v4 缩略图带着旧行为却仍然有效，
/// 人类看 view 里的纵拍 RAW 依旧是歪的（「感觉和以前一样」）。教训照旧：
/// **改渲染行为必须和抬版本号在同一个改动里完成**，中间任何一段产出的缓存都会变成毒缓存。
///
/// **第四次（v5 → v6，2026-09-24）**：编码格式从 JPEG 换成 **AVIF**（质量 90 / 4:4:4，
/// 人类定的全局口径）。这次**同时**改了 `render_sig` 的字面量与常量 —— 正是上面那条纪律。
/// 旧 JPEG 缓存变成孤儿，由 GC 收走（`cache.rs` 的 `drop_stale_signatures`）。
///
/// ⇒ 所以这条纪律的正确用法是：**同一个改动里，改了渲染行为就顺手抬版本**，
/// 不要「先合并渲染改动、之后再抬」（中间那段时间产出的缓存会带着旧行为却持有新签名）。
///
/// **第五次（v6 → v7，2026-09-25，M3-W4）**：缩略图 / 看图那条路接上了**可选阶段**
/// （镜头手动微调 / 降噪 / 锐化）与**动态反差** —— 此前缩略图只跑逐像素链，
/// 于是同一张照片在网格与编辑器里会是两张不同的图（`dynamicContrast` 完全没体现）。
/// v11（M3-W5）：成片几何先于网格展示夹取，并为紧裁切保留源像素。
// v12: lens profiles apply only after an explicit selection / auto-adjust action.
// v13: optional camera base curve before the editable user curve.
// v14: RGB16 display stages; quantize only after sharpen/LUT/geometry.
// v15: append an sRGB ICC to AVIF without changing or re-encoding AV1 samples.
// v16: 3:1 展示夹取先截后缩 —— 取源/解码尺寸把夹取算进去（极端长宽比不再糊，崔总 2026-10-10）。
pub const PIPELINE_VERSION: u32 = 16;

/// 缩略图尺度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SizeClass {
    /// 缩略图（长边 [`GRID_LONG_EDGE`]）—— 网格 tiles、胶片带、定稿卡片都吃它。
    Grid,
    /// 看图**载入占位**（长边 [`SCREEN_LONG_EDGE`]）。
    Screen,
    /// **真身**：该图自己的像素，不缩放（2026-10-11 崔总口径）。
    ///
    /// 看图（view / compare / fullscreen）的**终态**就是这一档；`Screen` 只是它到达之前的占位。
    /// 编码用 JPEG q92（20MP 的 AVIF 是秒级，实测见 `specs/view-true-image.md` §0.4）。
    Full,
}

/// [`SizeClass::Full`] 的「长边」：**不缩放**。
///
/// 取一个大于任何相机像素数的哨兵（当今天花板是 100MP / ~12240px），于是既有的
/// `resize_for_thumb`（不放大）与 `clamp(…, 8192)` 都自然退化成「原尺寸」——
/// 不需要给每一处缩放调用都开一个 `if Full` 分支。
/// （真身档的取源另算：见 [`geometry_input_edge`]。）
pub const FULL_LONG_EDGE: u32 = 16384;

/// 真身档的 JPEG 质量（q92 ≈ 8.9MB/20MP、编码 ~0.8s；再高质量只增体积）。
pub const FULL_JPEG_QUALITY: u8 = 92;

impl SizeClass {
    /// 稳定的字符串名（进数据库与缓存键，**不要改**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::Screen => "screen",
            Self::Full => "full",
        }
    }

    #[must_use]
    pub const fn long_edge(self) -> u32 {
        match self {
            Self::Grid => GRID_LONG_EDGE,
            Self::Screen => SCREEN_LONG_EDGE,
            Self::Full => FULL_LONG_EDGE,
        }
    }

    /// 所有尺度（GC、预热、统计遍历用）。
    #[must_use]
    pub const fn all() -> [Self; 3] {
        [Self::Grid, Self::Screen, Self::Full]
    }

    /// 这一档要不要按 [`MAX_DISPLAY_ASPECT`] 夹取？
    ///
    /// 只有缩略图档夹取；看图档（含真身）保持原始比例。
    #[must_use]
    pub const fn clamps_display_aspect(self) -> bool {
        matches!(self, Self::Grid)
    }

    /// 真身档（不缩放、不许用内嵌预览充数）。
    #[must_use]
    pub const fn is_full(self) -> bool {
        matches!(self, Self::Full)
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "grid" => Some(Self::Grid),
            "screen" => Some(Self::Screen),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

/// 渲染签名（编码格式 + 质量 + 管线版本 + 尺度）——**库内**（AVIF）用。
///
/// ⚠️ 这里的版本字面量必须与 [`PIPELINE_VERSION`] **同改** —— 常量管「新图用哪个版本」，
/// 字面量管「缓存键里写哪个版本」，两者不一致就会出现「改完算法、旧缓存仍命中」
/// （`render_sig_changes_with_pipeline_version` 这条测试就是钉它们的）。
#[must_use]
pub const fn render_sig(size: SizeClass) -> &'static str {
    match size {
        SizeClass::Grid => "avif-q90-grid-v16-embedded-v1",
        SizeClass::Screen => "avif-q90-screen-v16",
        // 真身档不走 `thumbs.db`（9MB/张的 JPEG 靠库内 `cache/full/<资产>/` 管），
        // 这个签名只是让本函数保持全函数；真身档自己的键在 `FullCache` 里（含管线版本）。
        SizeClass::Full => "jpeg-q92-full-v16",
    }
}

/// **库外（import / 来源目录）的渲染签名**：与 [`render_sig`] 同一套版本字面量，
/// 但格式段是 **JPEG q82**（`AGENTS.md` §2.24：来源目录不玩 AVIF）。
///
/// 两个签名分开是**故意的**：同一个文件先后被 import（库外）与 browse（库内）看，
/// 两边的图本来就是两份（格式不同、代价不同），不能互相冒充。
#[must_use]
pub const fn render_sig_source(size: SizeClass) -> &'static str {
    match size {
        SizeClass::Grid => "jpeg-q82-grid-v16-embedded-v1",
        SizeClass::Screen => "jpeg-q82-screen-v16",
        SizeClass::Full => "jpeg-q92-full-v16",
    }
}

/// **带编辑的渲染签名**：`<基础签名>+e<指纹>`（M3-W3）。
///
/// 没编辑过就是基础签名（与旧缓存完全一致 —— 老照片不会因为这次改动而重渲染）。
/// 编辑过则带上编辑栈的指纹：**改一个参数就换一个签名** ⇒ 自动绕开旧缓存。
///
/// ⚠️ 这种「动态签名」让 `drop_stale_signatures` 的 `NOT IN (...)` 判据失效
/// （它只认那几个静态签名）—— 所以那边的 GC 改成了**前缀匹配**（`cache.rs`）。
#[must_use]
pub fn render_sig_with_edit(size: SizeClass, edit: Option<&DevelopStack>) -> String {
    match edit {
        Some(stack) if !stack.is_empty() => {
            format!("{}+e{:016x}", render_sig(size), stack.signature())
        }
        _ => render_sig(size).to_string(),
    }
}

/// 一张渲染好的缩略图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    /// JPEG 字节（可以直接进 SQLite BLOB，也可以写文件）。
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// 是不是占位图（RAW 等）—— UI 可以据此加个角标。
    pub placeholder: bool,
}

/// **解码一个文件**（不缩放、不编码）——整仓唯一一份「文件 → 源像素 + 方向」的实现。
///
/// 两个消费方共用它：
///
/// | 消费方 | 要什么 | 做什么加工 |
/// | --- | --- | --- |
/// | [`render_file`]（网格 / 胶片带 / 看图的 JPEG） | 像素 | 摆正 → 缩放 → 编码 |
/// | `display::pixels`（编辑器的 GPU 纹理） | 像素 | 摆正 → 缩放 → RGBA8 |
///
/// 分出去的理由不是「想抽个层」，而是**方向（EXIF 1–8）那套优先级踩过三次坑**
/// （tiles 正 / view 歪、缓存毒图）。再抄一份就等着第四、五次。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeSpec {
    /// 长边上限；`None` = 原尺寸。
    ///
    /// **RAW 会把它透给 worker 在子进程里缩放**（一张 6000×4000 的 RGB 是 72MB，
    /// 跨进程搬完再缩很亏）；位图那条路不受它影响（先在内存里解出来，再缩）。
    pub max_edge: Option<u32>,
    /// 允许 RAW 走**内嵌预览**快路径（够大就用它）。
    pub allow_preview: bool,
    /// **强制走内嵌预览**（即便 `max_edge` 比网格档大）。
    ///
    /// 极端长宽比的小图要在 3:1 截取**之前**多取像素（见 [`display_aspect_scale`]），
    /// 但没必要为一张网格缩略图做一次真解码 —— 内嵌预览仍然快一两个数量级。
    pub embedded_only: bool,
}

impl DecodeSpec {
    /// 缩略图/屏幕档：RAW 在 worker 里缩、优先内嵌预览。
    #[must_use]
    pub const fn thumb(long_edge: u32) -> Self {
        Self {
            max_edge: Some(long_edge),
            allow_preview: true,
            embedded_only: false,
        }
    }

    /// 只读内嵌预览（不真解码）；`max_edge` 可以大于网格档。
    #[must_use]
    pub const fn embedded_preview(long_edge: u32) -> Self {
        Self {
            max_edge: Some(long_edge),
            allow_preview: true,
            embedded_only: true,
        }
    }

    /// 原尺寸完整解码（1:1 与将来的显影）。
    #[must_use]
    pub const fn full() -> Self {
        Self {
            max_edge: None,
            allow_preview: false,
            embedded_only: false,
        }
    }
}

/// 解出来的源像素（还没摆正、还没缩）。
#[derive(Debug, Clone)]
pub struct DecodedSource {
    pub image: DynamicImage,
    /// EXIF 方向（1–8）；`None` = 文件里读不到，按 1 处理。
    pub orientation: Option<u16>,
    /// 像素是从哪来的（RAW 才有区别：内嵌预览 / 真解码）——诊断与报告要用。
    pub source: PixelOrigin,
}

/// 像素的出处。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PixelOrigin {
    /// 位图（JPEG/PNG/TIFF…）解出来的。
    Bitmap,
    /// RAW 的内嵌 JPEG 预览（快路径）。
    RawEmbeddedPreview,
    /// RAW 完整解码（黑电平 / 白平衡 / 色彩矩阵）。
    RawDecoded,
    /// **我们自己的大图缓存**（`cache/full/<id>/latest-<base>-v<pipeline>.avif`）。
    ///
    /// 编辑器的过渡帧走它（进编辑先出图，`src-tauri/src/editor.rs`）——
    /// 它不是「照片解码」，而是「上一次渲染好的结果」。
    AvifCache,
}

/// 解码一个文件；解不开（不是图 / 相机不支持 / 文件损坏）返回 `Ok(None)`。
///
/// # Errors
/// 文件读不了、RAW worker 进程级故障（崩溃/超时/协议）——那些是**真错误**；
/// 「这张图解不开」不是。
pub fn decode_file(path: &Path, spec: DecodeSpec) -> Result<Option<DecodedSource>> {
    let kind = path
        .file_name()
        .map_or(MediaKind::Other, |n| kind_of_file(&n.to_string_lossy()));
    if kind == MediaKind::Raw {
        return decode_raw_file(path, spec);
    }

    let bytes = std::fs::read(path)?;
    /*
     * **方向必须在这里读**：竖拍照片（EXIF 方向 6/8）不摆正就会**躺着**显示 ——
     * 这是 2026-09-16 人类报的现象（「所有纵拍图全显示成横过来了」）。
     *
     * 用**已经读进来的字节**解析（`exif::read_bytes`），不额外开文件；
     * 而且这条路径只在缓存未命中时走，成本可以忽略（读头实测 ~0.02ms/张）。
     */
    let orientation = crate::media::exif::read_bytes(&bytes)
        .and_then(|data| data.orientation)
        .map(|raw| crate::media::meta::normalize_orientation(Some(raw)));
    let Ok(image) = image::load_from_memory(&bytes) else {
        return Ok(None); // 不是能解码的图像（RAW、损坏文件…）
    };
    Ok(Some(DecodedSource {
        image,
        orientation,
        source: PixelOrigin::Bitmap,
    }))
}

/// 渲染一个文件。
///
/// * **图像**（JPEG/PNG/TIFF…）直接解码；
/// * **RAW** 走 [`crate::raw`] 的 worker 进程（内嵌预览优先，见 `decode_raw_file`）；
/// * **都不是**（或解不开）返回 `Ok(None)` —— 调用方据此用占位图
///   （`memory/FUNCTION-REPOSITORY.md` §4.1）。
pub fn render_file(path: &Path, size: SizeClass) -> Result<Option<Thumb>> {
    render_file_with_edit(path, size, None, None)
}

/// **渲染一个文件，可选套用编辑栈**（M3-W3：缩略图 / 看图反映编辑结果）。
///
/// 编辑栈的管线在**缩放之后**跑：管线的调性与色度都是**逐像素**的，
/// 在缩小的图上跑数学上等价（重采样与逐像素操作几乎可交换），
/// 代价却只有全尺寸的百分之一 —— 缩略图不必为它付 24MP 的钱。
///
/// 编辑栈为空（没编辑过）时与 [`render_file`] 完全一样。
///
/// # Errors
/// 读不到文件 / 解码失败 / 编辑栈里的参数或曲线不合法。
pub fn render_file_with_edit(
    path: &Path,
    size: SizeClass,
    edit: Option<&DevelopStack>,
    lens: Option<&crate::develop::lens::LensCorrection>,
) -> Result<Option<Thumb>> {
    render_file_with_edit_and_lut(path, size, edit, lens, None, EncodeFormat::Avif)
}

pub fn render_file_with_edit_and_lut(
    path: &Path,
    size: SizeClass,
    edit: Option<&DevelopStack>,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
    format: EncodeFormat,
) -> Result<Option<Thumb>> {
    render_file_with_profiles(path, size, edit, lens, lut, format, |id, _| {
        Err(Error::Unsupported(format!("缩略图缺少输入 ICC：{}", id.as_str())))
    })
}

/// The same preview/cache entry point resolves frozen V2 color before resizing.
pub fn render_file_with_profiles(
    path: &Path, size: SizeClass, edit: Option<&DevelopStack>,
    lens: Option<&crate::develop::lens::LensCorrection>, lut: Option<&crate::develop::lut::Lut>,
    format: EncodeFormat,
    profile: impl FnMut(&crate::color::ProfileId, crate::color::icc::IccRole) -> Result<crate::color::icc::RgbIcc>,
) -> Result<Option<Thumb>> {
    if let Some(stack) = edit && let Some(color) = &stack.color {
        let (source, shot) = crate::display::output::decode_working_source(path, color, profile, crate::raw::DecodePriority::Normal)?;
        let edge = source_input_edge(size, source.dimensions(), stack.geometry);
        let source = crate::develop::working::WorkingReferenceImage::from_working(source.into_downscaled(edge))
            .map_err(|e| Error::Unsupported(e.to_string()))?;
        let working = crate::display::output::render_working(&source, stack, lens, shot, lut, 0)?;
        let target = crate::color::icc::srgb_icc().map_err(|e| Error::Unsupported(e.to_string()))?;
        let (w, h) = working.dimensions();
        let rgb = image::ImageBuffer::from_raw(w, h, working.to_rgb16_flat(&target).map_err(|e| Error::Unsupported(e.to_string()))?)
            .ok_or_else(|| Error::Unsupported("色彩缩略图尺寸与像素不符".into()))?;
        return encode_with_lut(DynamicImage::ImageRgb16(rgb), size, None, false, None, None, None, format).map(Some);
    }
    let kind = path
        .file_name()
        .map_or(MediaKind::Other, |n| kind_of_file(&n.to_string_lossy()));
    if kind == MediaKind::Raw {
        return render_raw_file(path, size, edit, lens, lut, format);
    }

    /*
     * 位图先试内嵌预览（EXIF 缩略图）—— 它够大就是最省的一条路。
     *
     * ⚠️ 顺序是**先出像素、比过尺寸，再编码**（2026-10-11 修）：这里以前是先
     * `render_bytes`（含编码）再比尺寸，而相机的内嵌图常见只有 160×120 —— 
     * 不够 384 档就整张丢掉，白烧一次编码（实测 ~118 ms/张，AVIF）。
     * 现在不够大的话一次编码都不花，直接走完整解码。
     */
    let embedded = if size.clamps_display_aspect() && edit.is_none_or(DevelopStack::is_empty) {
        crate::media::embedded::read(path).and_then(|embedded| {
            let img = image::load_from_memory(&embedded.bytes).ok()?;
            prepare_pixels(img, size, embedded.orientation, None, None, None).ok()
        })
    } else {
        None
    };
    // 内嵌图够大（3:1 截取后仍是满档）就直接用；不够大说明源不够，
    // 宁可完整解码（只慢这一次，结果进缓存）—— 先缩后截会只剩几十像素（崔总 2026-10-10）。
    if let Some(pixels) = &embedded
        && pixels.width().max(pixels.height()) >= size.long_edge()
    {
        return thumb_from_pixels(pixels, size, format).map(Some);
    }

    let bytes = std::fs::read(path)?;
    /*
     * **方向必须在这里读**：竖拍照片（EXIF 方向 6/8）不摆正就会**躺着**显示 ——
     * 这是 2026-09-16 人类报的现象（「所有纵拍图全显示成横过来了」）。
     *
     * 用**已经读进来的字节**解析（`exif::read_bytes`），不额外开文件；
     * 而且这条路径只在缓存未命中时走，成本可以忽略（读头实测 ~0.02ms/张）。
     */
    let orientation = crate::media::exif::read_bytes(&bytes)
        .and_then(|data| data.orientation)
        .map(|raw| crate::media::meta::normalize_orientation(Some(raw)));
    let full = render_bytes_with_edit_and_lut(&bytes, size, orientation, edit, lens, lut, format)?;
    // 完整解码没出图（例如只有内嵌图可用的合成 / 损坏文件）→ 退回内嵌结果（这时才编码）
    if full.is_none()
        && let Some(pixels) = &embedded {
            return thumb_from_pixels(pixels, size, format).map(Some);
        }
    Ok(full)
}

/// 给 RAW 缩放时多要的倍数：最终尺寸的 Lanczos 由 [`encode`] 在小图上做，
/// 这里只要给出略大的源（两道降采样比一次大跨度缩放更干净）。
const RAW_OVERSAMPLE: u32 = 2;

/// 裁切前需要保留足够源像素：成片若只占原图 1/4，先缩到目标长边会让结果只剩 1/4 尺寸。
/// RAW 与位图共用这个口径，最后仍由 `encode` 收至请求的档位。
///
/// **只管编辑栈的几何**（用户裁切）；网格的 3:1 展示夹取另算 —— 见 [`source_input_edge`]。
fn geometry_input_edge(
    size: SizeClass,
    source: (u32, u32),
    geometry: Option<crate::develop::geometry::EditGeometry>,
) -> u32 {
    // 真身档：**源有多少就给多少**（不过采样、不受 8192 夹取 —— 真身就是真身）。
    if size.is_full() {
        return source.0.max(source.1).max(1);
    }
    let base = size.long_edge().saturating_mul(RAW_OVERSAMPLE);
    let Some(geometry) = geometry.filter(|g| !g.is_identity()) else {
        return base;
    };
    let crop = geometry.output_rect(source);
    let fraction = crop.width.min(crop.height).max(0.01);
    ((base as f32 / fraction).ceil() as u32).clamp(base, 8192)
}

/// **取源**要多少长边像素（解码 / 先缩一次之前用）：几何裁切 **×** 3:1 展示夹取。
///
/// 两个「裁切」都要算进去，否则先缩后截会只剩几个像素：
/// * 编辑栈的成片几何（[`geometry_input_edge`]）；
/// * 网格 / 胶片带的 3:1 展示夹取（[`display_aspect_scale`]，1:10 的图截完只剩 1/3.3）。
///
/// 最后仍由 `encode` 在小图上截取并收至请求的档位（RAW 与 V2 共用这个口径）。
fn source_input_edge(
    size: SizeClass,
    source: (u32, u32),
    geometry: Option<crate::develop::geometry::EditGeometry>,
) -> u32 {
    let base = geometry_input_edge(size, source, geometry);
    if !size.clamps_display_aspect() {
        return base;
    }
    let scale = display_aspect_scale(source.0, source.1).max(1.0);
    ((base as f32 * scale).ceil() as u32).clamp(base, 8192)
}

/// RAW 的渲染（= [`decode_raw_file`] + 编码）。
fn render_raw_file(
    path: &Path,
    size: SizeClass,
    edit: Option<&DevelopStack>,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
    format: EncodeFormat,
) -> Result<Option<Thumb>> {
    let source_size = crate::media::meta::read_photo_meta(path)
        .ok()
        .map(|meta| (meta.width, meta.height))
        .unwrap_or((1, 1));
    if size.clamps_display_aspect() && edit.is_none_or(DevelopStack::is_empty) {
        // 内嵌预览快路径：取源尺寸要把 3:1 展示夹取算进去，否则极端长宽比只剩几十像素
        //（崔总 2026-10-10）；仍然只读内嵌预览，不为一张网格图真解码。
        let want = source_input_edge(size, source_size, None).max(1);
        if let Some(decoded) = decode_raw_file(path, DecodeSpec::embedded_preview(want))? {
            return encode_with_format(decoded.image, size, decoded.orientation, false, None, None, format).map(Some);
        }
        // 没有内嵌预览 → 落到下面的常规请求（必要时真解码），与老行为一致
    }
    /*
     * **真身档：完整解码，禁止内嵌预览**（2026-10-11 崔总口径）。
     *
     * 内嵌预览是**机内 JPEG**：在它上面套参数会让「看图里的这张」与编辑视口
     * 成为两张不同的照片（与下面 `latest` 那条同一个道理），而且分辨率也不够
     *（它就是 1920 那一档）。所以真身档走 `DecodeRequest::full`（不限长边、不要预览）。
     */
    if size.is_full() {
        if let Some(stack) = edit.filter(|stack| !stack.is_empty()) {
            let request = crate::raw::DecodeRequest::full(path);
            let _permit = crate::raw::worker::acquire(crate::raw::DecodePriority::Normal);
            let mut decoded = crate::raw::worker::shared()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .decode_linear(&request)
                .map_err(|e| Error::Unsupported(format!("RAW 线性解码失败：{e}")))?;
            let shot_k = decoded.as_shot_temperature;
            decoded.orientation = crate::media::exif::raw_orientation(path, decoded.orientation);
            let linear = LinearImage::from_raw16(decoded)
                .ok_or_else(|| Error::Unsupported("RAW 线性像素尺寸不合法".to_string()))?;
            let rgb = apply_develop_linear(&linear, stack, lens, shot_k, lut)?;
            return encode_with_format(DynamicImage::ImageRgb8(rgb), size, None, false, None, None, format).map(Some);
        }
        let Some(decoded) = decode_raw_file(path, DecodeSpec::full())? else {
            return Ok(None);
        };
        return encode_with_format(decoded.image, size, decoded.orientation, false, None, lens, format).map(Some);
    }
    let want = source_input_edge(size, source_size, edit.and_then(|stack| stack.geometry)).max(1);
    // latest 基于 RAW 时必须从传感器线性像素显影。内嵌 JPEG 是机内 SOOC，
    // 在它上面套参数会让总览/胶片带与编辑视口成为两张不同的照片。
    if let Some(stack) = edit.filter(|stack| !stack.is_empty()) {
        let request = crate::raw::DecodeRequest::thumb(path, want).with_preview(false);
        // 缩略图/胶片带是 Normal 档（§4.4）
        let _permit = crate::raw::worker::acquire(crate::raw::DecodePriority::Normal);
        let mut decoded = crate::raw::worker::shared()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .decode_linear(&request)
            .map_err(|e| Error::Unsupported(format!("RAW 线性解码失败：{e}")))?;
        let shot_k = decoded.as_shot_temperature;
        decoded.orientation = crate::media::exif::raw_orientation(path, decoded.orientation);
        let linear = LinearImage::from_raw16(decoded)
            .ok_or_else(|| Error::Unsupported("RAW 线性像素尺寸不合法".to_string()))?
            .downscaled_to(want);
        let rgb = apply_develop_linear(&linear, stack, lens, shot_k, lut)?;
        return encode_with_format(DynamicImage::ImageRgb8(rgb), size, None, false, None, None, format).map(Some);
    }
    let Some(decoded) = decode_raw_file(path, DecodeSpec::thumb(want))? else {
        return Ok(None);
    };
    encode_with_format(decoded.image, size, decoded.orientation, false, None, lens, format).map(Some)
}

/// RAW 的解码：走 worker 进程（`AGENTS.md` §6.3：解码必须在独立进程里）。
///
/// 解不开（相机不支持 / 文件损坏 / worker 崩了）**不是错误** ——
/// 一张解不开的 RAW 不该让整个导入挂掉，所以这里 `Ok(None)`、由调用方用占位图兜底。
fn decode_raw_file(path: &Path, spec: DecodeSpec) -> Result<Option<DecodedSource>> {
    let request = match spec.max_edge {
        // 强制内嵌（极端长宽比的小图要在截取前多取像素，但不值得为它真解码）
        Some(max_edge) if spec.embedded_only =>
            crate::raw::DecodeRequest::embedded(path, max_edge.max(1)),
        Some(max_edge) if spec.allow_preview && max_edge <= GRID_LONG_EDGE =>
            crate::raw::DecodeRequest::embedded(path, max_edge.max(1)),
        Some(max_edge) => crate::raw::DecodeRequest::thumb(path, max_edge.max(1)),
        None => crate::raw::DecodeRequest::full(path),
    }
    .with_preview(spec.allow_preview);

    // 共享一条解码管道（理由见 `raw::worker::shared`）；缩略图/预览都是 Normal 档
    let decoded = {
        let _permit = crate::raw::worker::acquire(crate::raw::DecodePriority::Normal);
        let mut worker = crate::raw::worker::shared()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match worker.decode(&request) {
            Ok(image) => image,
            Err(e) => {
                log::warn!(
                    "[thumb] RAW 解码失败（改用占位图）：{} —— {e}",
                    path.display()
                );
                return Ok(None);
            }
        }
    };

    let source = match decoded.source {
        crate::raw::PixelSource::EmbeddedPreview => PixelOrigin::RawEmbeddedPreview,
        crate::raw::PixelSource::Decoded => PixelOrigin::RawDecoded,
    };
    let Some(buffer) = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb) else {
        return Ok(None);
    };

    /*
     * 方向：**文件头优先**，读不到才用 worker 报的。
     * 规则与优先级反过一次的教训都在 `media::exif::raw_orientation`（编辑器的线性解码
     * 也用同一个函数，不许各写一套）。
     */
    let orientation = if request.embedded_only { decoded.orientation }
        else { crate::media::exif::raw_orientation(path, decoded.orientation) };

    Ok(Some(DecodedSource {
        image: DynamicImage::ImageRgb8(buffer),
        orientation,
        source,
    }))
}

/// 从内存渲染；`orientation` 是 EXIF 的 1..8（给了就摆正）。
pub fn render_bytes(
    bytes: &[u8],
    size: SizeClass,
    orientation: Option<u16>,
) -> Result<Option<Thumb>> {
    render_bytes_with_edit(bytes, size, orientation, None, None)
}

/// A preview already contains the edit and orientation: only resize/crop it.
/// Missing or corrupt disposable previews use the existing source renderer.
pub fn render_preview_or_else(
    preview: Option<&[u8]>,
    size: SizeClass,
    fallback: impl FnOnce() -> Result<Option<Thumb>>,
) -> Result<Option<Thumb>> {
    if let Some(bytes) = preview
        && let Some(thumb) = render_bytes(bytes, size, None)?
    {
        return Ok(Some(thumb));
    }
    fallback()
}

/// 从内存渲染（可带编辑栈）—— [`render_bytes`] 的完整形态。
///
/// # Errors
/// 解码失败（不是图）返回 `Ok(None)`；编辑栈不合法才是 `Err`。
pub fn render_bytes_with_edit(
    bytes: &[u8],
    size: SizeClass,
    orientation: Option<u16>,
    edit: Option<&DevelopStack>,
    lens: Option<&crate::develop::lens::LensCorrection>,
) -> Result<Option<Thumb>> {
    render_bytes_with_edit_and_lut(bytes, size, orientation, edit, lens, None, EncodeFormat::Avif)
}

pub fn render_bytes_with_edit_and_lut(
    bytes: &[u8],
    size: SizeClass,
    orientation: Option<u16>,
    edit: Option<&DevelopStack>,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
    format: EncodeFormat,
) -> Result<Option<Thumb>> {
    let Ok(img) = image::load_from_memory(bytes) else {
        return Ok(None); // 不是能解码的图像（RAW、损坏文件…）
    };
    encode_with_lut(img, size, orientation, false, edit, lens, lut, format).map(Some)
}

/// 网格/胶片带小图的**最大展示宽高比**（两侧都算：3:1 与 1:3）。
///
/// 人类 2026-09-16 定的口径：**超出就居中截取**。理由是网格是「浏览」，
/// 全景图按原始比例塞进正方外框会缩成一条细缝，谁也看不清；
/// 截取到 3:1 之后至少看得出是全景、也有内容可看。
///
/// **只作用于缩略图档**（`Grid`）—— 看图用的 `Screen` 档必须保持原始比例，
/// 否则全景照片在查看器里就看不成完整的了。
pub const MAX_DISPLAY_ASPECT: f64 = 3.0;

/// 按最大宽高比算出**居中截取**的矩形（`x, y, w, h`）。
///
/// 比例在范围内 → 原样返回；超出 → 只砍长的那一边、两边各砍一半（居中）。
/// 纯函数，好测；`u32` 进出，不碰像素。
#[must_use]
pub fn clamp_rect(width: u32, height: u32, max_aspect: f64) -> (u32, u32, u32, u32) {
    if width == 0 || height == 0 || !max_aspect.is_finite() || max_aspect <= 0.0 {
        return (0, 0, width, height);
    }
    let aspect = f64::from(width) / f64::from(height);
    if aspect > max_aspect {
        // 太宽：砍宽度
        let target = (f64::from(height) * max_aspect).round() as u32;
        let target = target.clamp(1, width);
        return ((width - target) / 2, 0, target, height);
    }
    if aspect < 1.0 / max_aspect {
        // 太高：砍高度。要让宽高比达到 1:max，高度取 `宽 × max`
        //（这里曾经写成除以 max —— 纯函数测试当场抓到）
        let target = (f64::from(width) * max_aspect).round() as u32;
        let target = target.clamp(1, height);
        return (0, (height - target) / 2, width, target);
    }
    (0, 0, width, height)
}

/// **3:1 展示夹取会让长边缩短多少倍**（≥ 1；比例在范围内 = 1）。
///
/// 小图的口径是「先按 3:1 居中截取、再缩到目标档」。所以**取源 / 解码的尺寸必须把这一刀
/// 算进去**：369×3693 这种 1:10 的图，截完只剩约 1/3.3 的长边 —— 若先把源缩到 384 再截，
/// 结果只剩 38×114 像素，`resize_for_thumb` 又不放大，铺满 tile 就是糊的
/// （崔总 2026-10-10 报的现象；这条纯函数就是给取源尺寸用的）。
#[must_use]
pub fn display_aspect_scale(width: u32, height: u32) -> f32 {
    if width == 0 || height == 0 {
        return 1.0;
    }
    let (_, _, kept_w, kept_h) = clamp_rect(width, height, MAX_DISPLAY_ASPECT);
    let long = width.max(height) as f64;
    let kept_long = kept_w.max(kept_h) as f64;
    if kept_long <= 0.0 || kept_long >= long {
        return 1.0;
    }
    (long / kept_long) as f32
}

/// 按最大展示宽高比居中截取（`Grid` 用；`Screen` 不调它）。
#[must_use]
pub fn clamp_display_aspect(img: DynamicImage, max_aspect: f64) -> DynamicImage {
    let (x, y, width, height) = clamp_rect(img.width(), img.height(), max_aspect);
    if (width, height) == (img.width(), img.height()) {
        return img;
    }
    img.crop_imm(x, y, width, height)
}

/// 把编辑栈套到一张**显示像素**（8bit sRGB）上。
///
/// 输入是已经解出来、缩好尺寸的显示数据 —— 所以这一步只做「线性化 → 管线 → 回到 8bit」。
/// 对 JPG / RAW 内嵌预览来说它是**准线性**（8bit 里本来就没有更多信息），
/// 与编辑器里那条 JPG 路同一个口径（`memory/FUTURE.md` C7 记着收敛点）。
///
/// # Errors
/// 栈里的参数 / 曲线不合法（**不静默跳过** —— 那会让人看到一张「没生效」的图而不知道为什么）。
fn apply_develop(
    img: &image::RgbImage,
    stack: &DevelopStack,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
) -> Result<image::RgbImage> {
    let (width, height) = img.dimensions();
    let Some(linear) = LinearImage::from_srgb8(width, height, img.as_raw()) else {
        return Err(Error::Unsupported(
            "编辑渲染：像素长度与尺寸对不上".to_string(),
        ));
    };
    apply_develop_linear(&linear, stack, lens, None, lut)
}

/// 位图与真 RAW 共用同一套编辑阶段；RAW 的 as-shot 色温由本次解码优先提供。
fn apply_develop_linear(
    linear: &LinearImage,
    stack: &DevelopStack,
    lens: Option<&crate::develop::lens::LensCorrection>,
    decoded_as_shot_k: Option<f32>,
    lut: Option<&crate::develop::lut::Lut>,
) -> Result<image::RgbImage> {
    let image = crate::display::output::render_linear(linear, stack, lens, decoded_as_shot_k, lut, 0)?;
    image::RgbImage::from_raw(image.width(), image.height(),
        crate::develop::pipeline::quantize_rgb8(image.as_raw()))
        .ok_or_else(|| Error::Unsupported("显示像素尺寸不一致".into()))
}

/// 缩放后编码成缓存图 —— 库内走 AVIF（见 [`encode_avif`]），库外走 JPEG（[`EncodeFormat`]）。
///
/// 缩放走 [`resize_for_thumb`]（盒式快降，2026-10-11 起不再做 Lanczos3 精修）。
pub fn encode(
    img: DynamicImage,
    size: SizeClass,
    orientation: Option<u16>,
    placeholder: bool,
    edit: Option<&DevelopStack>,
    lens: Option<&crate::develop::lens::LensCorrection>,
) -> Result<Thumb> {
    encode_with_format(img, size, orientation, placeholder, edit, lens, EncodeFormat::Avif)
}

/// 同 [`encode`]，但**编码格式由场景给**（库外 import 传 [`EncodeFormat::Jpeg`]）。
pub fn encode_with_format(
    img: DynamicImage,
    size: SizeClass,
    orientation: Option<u16>,
    placeholder: bool,
    edit: Option<&DevelopStack>,
    lens: Option<&crate::develop::lens::LensCorrection>,
    format: EncodeFormat,
) -> Result<Thumb> {
    encode_with_lut(img, size, orientation, placeholder, edit, lens, None, format)
}

pub fn encode_with_lut(
    img: DynamicImage,
    size: SizeClass,
    orientation: Option<u16>,
    placeholder: bool,
    edit: Option<&DevelopStack>,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
    format: EncodeFormat,
) -> Result<Thumb> {
    let result = prepare_pixels(img, size, orientation, edit, lens, lut)?;
    let (width, height) = result.dimensions();
    let data = encode_pixels(result.as_raw(), width, height, size, format)?;
    Ok(Thumb {
        data,
        width,
        height,
        placeholder,
    })
}

/// 出图管线的**前半段**：方向 → 3:1 夹取 → 缩放 →（可选）编辑栈，产出可编码的 RGB。
///
/// 单独拆出来只为一件事：**先出像素、比过尺寸，再决定要不要编码**。
/// 内嵌预览常常小于目标档（相机实测全是 160×120），先编码再发现不够就是白烧一次编码
/// （2026-10-11 实测 ~118 ms/张，见 `implementations/2026-10-11_import-tiles-diagnosis.md`）。
///
/// # Errors
/// 编辑栈里的参数 / 曲线不合法。
pub fn prepare_pixels(
    img: DynamicImage,
    size: SizeClass,
    orientation: Option<u16>,
    edit: Option<&DevelopStack>,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
) -> Result<image::RgbImage> {
    let img = match orientation {
        Some(o) if o != 1 => apply_orientation(&img, o),
        _ => img,
    };
    /*
     * 小图（网格 / 胶片带）先按 3:1 居中截取，再缩放；
     * `Screen`（看图）不截取 —— 查看器必须看到完整照片（2026-09-16 口径）。
     */
    let geometry = edit
        .and_then(|stack| stack.geometry)
        .filter(|g| !g.is_identity());
    // 成片几何先于网格的 3:1 展示夹取：编辑框以整张原图为基准。
    // 紧裁切要先多取像素，最后再收至目标尺度。
    let img = if geometry.is_none() && size.clamps_display_aspect() {
        clamp_display_aspect(img, MAX_DISPLAY_ASPECT)
    } else {
        img
    };
    let source_edge = if geometry.is_some() {
        geometry_input_edge(size, (img.width(), img.height()), geometry)
    } else {
        size.long_edge()
    };
    let resized = resize_for_thumb(img, source_edge);
    let rgb = resized.to_rgb8();
    let developed = match edit {
        Some(stack) if !stack.is_empty() => apply_develop(&rgb, stack, lens, lut)?,
        _ => rgb,
    };
    Ok(if geometry.is_some() {
        resize_for_thumb(DynamicImage::ImageRgb8(developed), size.long_edge()).to_rgb8()
    } else {
        developed
    })
}

/// 编码「准备好的像素」——**缓存图唯一的编码出口**（格式由场景给，见 [`EncodeFormat`]）。
///
/// # Errors
/// 编码器内部错误。
fn encode_pixels(
    rgb: &[u8],
    width: u32,
    height: u32,
    size: SizeClass,
    format: EncodeFormat,
) -> Result<Vec<u8>> {
    if size.is_full() {
        // 真身档恒 JPEG（20MP 的 AVIF 编码是秒级，JPEG 是 ~0.8s；`specs/view-true-image.md`）
        return encode_jpeg_bytes(rgb, width, height, FULL_JPEG_QUALITY);
    }
    match format {
        EncodeFormat::Jpeg => encode_jpeg_bytes(rgb, width, height, JPEG_THUMB_QUALITY),
        EncodeFormat::Avif => encode_avif(rgb, width, height),
    }
}

/// 像素 → 一张缓存图（内嵌预览快路径与占位图用）——[`prepare_pixels`] 的下半段。
fn thumb_from_pixels(
    pixels: &image::RgbImage,
    size: SizeClass,
    format: EncodeFormat,
) -> Result<Thumb> {
    let (width, height) = pixels.dimensions();
    Ok(Thumb {
        data: encode_pixels(pixels.as_raw(), width, height, size, format)?,
        width,
        height,
        placeholder: false,
    })
}

/// JPEG 编码（只给真身档用；其余档仍是 AVIF）。
///
/// # Errors
/// 编码器失败（尺寸不合法 / 内存）。
fn encode_jpeg_bytes(rgb: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, quality);
    encoder
        .encode(rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|error| Error::Unsupported(format!("JPEG 编码失败：{error}")))?;
    Ok(data)
}

/// 缩到长边 `long`（不放大）：**一条盒式快降走到底**。
///
/// # 为什么去掉了 Lanczos3 精修（崔总 2026-10-11 定）
///
/// 以前是两段式：盒式快降到 2× 目标，再用 Lanczos3 精修最后一段
/// （实测 45 ms/张；直接 Lanczos3 是 191 ms —— 见 `examples/thumb-bench.rs`）。
/// 崔总的口径是**能多快就多快**：缩略图是「先出来」的东西，画质可以让步，
/// 于是精修那一步去掉，只留盒式（`thumbnail` 的整数快路径）。
///
/// 大幅缩小时两者观感差别很小（高频本就丢掉了）；真要再快一档得换解码器
/// —— libjpeg-turbo 的 1/8 DCT 缩放解码登记在 `memory/FUTURE.md`。
#[must_use]
pub fn resize_for_thumb(img: DynamicImage, long: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w.max(h) <= long {
        return img; // 已经比目标小：不放大（放大只会更糊，还更费空间）
    }
    img.thumbnail(long, long)
}

/// 按 EXIF orientation 摆正（1..8）。
///
/// 相机的方向传感器把「怎么拿的」记在这里：竖着拍的照片其像素是横的，
/// 不加这一步网格里就会出现一片躺倒的照片。
#[must_use]
pub fn apply_orientation(img: &DynamicImage, orientation: u16) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img.clone(), // 1 与未知值：原样
    }
}

/// 生成占位图：深底 + 「RAW」字样，**不依赖字体文件**。
///
/// 现在只有「真解不开」才用得上（相机不支持、文件损坏）—— RAW 正常会走
/// [`crate::raw`] 的 worker 解码（见 [`render_file`]）。
///
/// 字体是手写的 5×7 点阵（只有 R / A / W 三个字母，够用）。
pub fn placeholder(kind: MediaKind, size: SizeClass) -> Result<Thumb> {
    let img = placeholder_image(kind, size);
    encode(DynamicImage::ImageRgb8(img), size, None, true, None, None)
}

/// **画**占位图（纯像素，不编码）。
///
/// 与 [`placeholder`] 分开是为了可测：AVIF 只有编码器（纯 Rust 那条路没有解码器），
/// 想断言「字真的画上去了」就只能在这一层看像素。
#[must_use]
pub fn placeholder_image(kind: MediaKind, size: SizeClass) -> RgbImage {
    // 占位图不可能是真身档（它只在网格/胶片带/屏幕档兜底）；夹一道保险，
    // 免得将来谁拿 `Full` 要占位图时画出一张 16384 像素的空图。
    let long = size.long_edge().min(SCREEN_LONG_EDGE);
    // 占位图按 3:2 画（相机的常见比例），再按尺度等比
    let (w, h) = (long, long * 2 / 3);
    let mut img = RgbImage::from_pixel(w, h, Rgb(BG));

    // 网格里加一圈淡淡的描边，避免浅色主题下与背景糊在一起
    for x in 0..w {
        img.put_pixel(x, 0, Rgb(EDGE));
        img.put_pixel(x, h - 1, Rgb(EDGE));
    }
    for y in 0..h {
        img.put_pixel(0, y, Rgb(EDGE));
        img.put_pixel(w - 1, y, Rgb(EDGE));
    }

    let text = match kind {
        MediaKind::Raw => "RAW",
        MediaKind::Image => "IMG",
        MediaKind::Other => "?",
    };
    draw_text(&mut img, text, FONT_SCALE.max(1));
    img
}

// 占位图配色（这里**故意不引 tokens.css**：它是画布内容不是 UI 组件，
// 但取值与 memory/DESIGN.md 的 `--surface-bar` / `--fg-3` 保持一致，改了要同步。）
const BG: [u8; 3] = [0x2a, 0x2d, 0x33];
const EDGE: [u8; 3] = [0x3a, 0x3e, 0x46];
const FG: [u8; 3] = [0xb6, 0xb0, 0xaf];

/// 点阵放大的倍数（小尺度下用大字母，大尺度下用大字母也一样清楚）。
const FONT_SCALE: u32 = 6;

/// 5×7 点阵字体（只含占位图需要的字母）。
fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c {
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        '?' => [
            0b01110, 0b10001, 0b00010, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
        _ => return None,
    })
}

/// 把一串字画在图片正中间。
fn draw_text(img: &mut RgbImage, text: &str, scale: u32) {
    let (gw, gh) = (5u32, 7u32);
    let spacing = scale;
    let total_w: u32 = text.chars().filter(|c| glyph(*c).is_some()).count() as u32
        * (gw * scale + spacing)
        - spacing.max(1);
    let total_h = gh * scale;
    let x0 = img.width().saturating_sub(total_w) / 2;
    let y0 = img.height().saturating_sub(total_h) / 2;

    let mut cursor = x0;
    for c in text.chars() {
        let Some(rows) = glyph(c) else { continue };
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..gw {
                if bits & (1 << (gw - 1 - rx)) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = cursor + rx * scale + dx;
                        let y = y0 + ry as u32 * scale + dy;
                        if x < img.width() && y < img.height() {
                            img.put_pixel(x, y, Rgb(FG));
                        }
                    }
                }
            }
        }
        cursor += gw * scale + spacing;
    }
}

/// 给「不是图像」的字节算个稳定的图（调试用：把哈希画成条纹）。本阶段不用。
#[allow(dead_code)]
pub(crate) fn color_of(bytes: &[u8]) -> Rgb<u8> {
    let mut h: u32 = 2166136261;
    for b in bytes.iter().take(64) {
        h = (h ^ u32::from(*b)).wrapping_mul(16777619);
    }
    #[allow(clippy::cast_possible_truncation)]
    Rgb([(h >> 16) as u8, (h >> 8) as u8, h as u8])
}

/// **AVIF 字节是不是一张「像样的」图**（校验用：缓存里存的东西别是坏的）。
///
/// 认 ISO-BMFF 的 `ftyp` box：偏移 4 处是 `ftyp`，主品牌是 `avif` / `avis`（序列）
/// / `mif1`（兼容品牌）。**不看文件内容能不能解** —— 那是解码器的事。
#[must_use]
pub fn is_valid_avif(bytes: &[u8]) -> bool {
    bytes.len() > 64
        && bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(&bytes[8..12], b"avif" | b"avis" | b"mif1")
}

/// JPEG 字节是不是一张「像样的」图（**导出**与互操作那条路还用 JPEG）。
#[must_use]
pub fn is_valid_jpeg(bytes: &[u8]) -> bool {
    matches!(image::guess_format(bytes), Ok(ImageFormat::Jpeg)) && bytes.len() > 64
}

/// 校验一个 `ExtendedColorType`（`image` 0.25 需要，避免 elsewhere 手拼）。
#[must_use]
pub fn rgb8_layout() -> ExtendedColorType {
    ExtendedColorType::Rgb8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手动基准：显式 nclx 的 RGB 内部模型有可见耗时和体积回归，
    /// 保留数据供后续设计低成本容器标记方案时比较，不作为默认编码。
    #[test]
    #[ignore]
    fn bench_avif_srgb_marker_cost() {
        use image::ImageEncoder;
        use std::time::Instant;
        let (width, height) = (1920_u32, 1280_u32);
        let rgb: Vec<u8> = (0..width as usize * height as usize * 3)
            .map(|index| (index.wrapping_mul(71) % 256) as u8)
            .collect();
        let mut legacy = Vec::new();
        let started = Instant::now();
        image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut legacy, AVIF_SPEED, AVIF_QUALITY)
            .with_num_threads(Some(1))
            .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
            .unwrap();
        let legacy_elapsed = started.elapsed();
        let start=Instant::now();
        let tagged=crate::media::isobmff::attach_icc(&legacy,crate::color::icc::srgb_icc().unwrap().bytes()).unwrap();
        eprintln!("ICC container only {:?}/{} bytes; AV1 payload retained",start.elapsed(),tagged.len());
        let mut marked = Vec::new();
        let started = Instant::now();
        image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut marked, AVIF_SPEED, AVIF_QUALITY)
            .with_colorspace(image::codecs::avif::ColorSpace::Srgb)
            .with_num_threads(Some(1))
            .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
            .unwrap();
        let marked_elapsed = started.elapsed();
        assert!(marked.windows(8).any(|window| window == b"nclx\0\x01\0\x0d"));
        eprintln!("1920×1280 AVIF old {:?}/{} bytes, explicit sRGB {:?}/{} bytes", legacy_elapsed, legacy.len(), marked_elapsed, marked.len());
    }

    fn jpeg_of(w: u32, h: u32, color: [u8; 3]) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, Rgb(color));
        let mut buf = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 90);
        enc.encode_image(&img).unwrap();
        buf
    }

    /// 往一张真 JPEG 里插一段只含 `Orientation` 的 EXIF（APP1）——
    /// 用来验「竖拍照片必须摆正」（人类 2026-09-16 报的现象）。
    fn jpeg_with_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        let mut tiff: Vec<u8> = Vec::new();
        tiff.extend_from_slice(b"MM\x00\x2a");
        tiff.extend_from_slice(&8u32.to_be_bytes());
        tiff.extend_from_slice(&1u16.to_be_bytes()); // 一个条目
        tiff.extend_from_slice(&0x0112u16.to_be_bytes()); // Orientation
        tiff.extend_from_slice(&3u16.to_be_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_be_bytes());
        let mut slot = [0u8; 4];
        slot[..2].copy_from_slice(&orientation.to_be_bytes());
        tiff.extend_from_slice(&slot);
        tiff.extend_from_slice(&0u32.to_be_bytes()); // 没有下一个 IFD

        let mut payload = Vec::from(*b"Exif\x00\x00");
        payload.extend_from_slice(&tiff);
        let mut segment = vec![0xFF, 0xE1];
        segment.extend_from_slice(&u16::try_from(payload.len() + 2).unwrap().to_be_bytes());
        segment.extend_from_slice(&payload);

        // APP1 要插在 SOI（FF D8）之后
        let mut out = Vec::from(&jpeg[..2]);
        out.extend_from_slice(&segment);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    fn png_of(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, Rgb([10, 20, 30]));
        let mut buf = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    // ---------- 尺度与签名 ----------

    #[test]
    fn size_class_contract() {
        assert_eq!(SizeClass::Grid.long_edge(), 384);
        // 看图档（M1-9 之后加）：屏幕级别的大图
        assert_eq!(SizeClass::Screen.long_edge(), 1920);
        // 真身档（2026-10-11 加）：不缩放 —— 「长边」是个大于任何相机的哨兵
        assert_eq!(SizeClass::Full.long_edge(), FULL_LONG_EDGE);
        assert!(FULL_LONG_EDGE > 12240, "要大于今天最大的相机像素数（100MP ≈ 12240px）");
        assert!(SizeClass::Full.is_full());
        assert!(!SizeClass::Screen.is_full());
        assert_eq!(SizeClass::all().len(), 3);
        for s in SizeClass::all() {
            assert_eq!(SizeClass::parse(s.as_str()), Some(s));
            assert!(render_sig(s).contains(s.as_str()), "签名里要能看出尺度");
        }
        assert_eq!(SizeClass::parse(""), None);
        assert_eq!(SizeClass::parse("不认识的档"), None);
    }

    /// **真身档不许缩**（2026-10-11 崔总口径：「禁止再出现拿假图来充看图界面的功能」）：
    /// 一张比屏幕档还大的位图，`Full` 出来必须还是它自己的像素尺寸；
    /// 同一张图的 `Screen` 则按老规矩缩到 1920 —— 两个档的差别就在这里。
    #[test]
    fn full_class_renders_the_source_pixels_without_downscaling() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.jpg");
        std::fs::write(&path, jpeg_of(2400, 1800, [40, 90, 160])).unwrap();
        let full = render_file(&path, SizeClass::Full).unwrap().expect("真身档");
        assert_eq!((full.width, full.height), (2400, 1800), "不缩 —— 就是原像素");
        assert_eq!(&full.data[..2], &[0xff, 0xd8], "真身档是 JPEG（AVIF 编码太慢）");
        let screen = render_file(&path, SizeClass::Screen).unwrap().expect("屏幕档");
        assert_eq!((screen.width, screen.height), (1920, 1440), "屏幕档仍缩到 1920");
    }

    /// 真身档 + 裁切：出的是**裁切后的原像素**（不是再缩到某个长边）。
    #[test]
    fn full_class_keeps_the_crop_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crop.jpg");
        std::fs::write(&path, jpeg_of(2400, 1800, [10, 120, 200])).unwrap();
        let mut stack = DevelopStack {
            geometry: Some(crate::develop::geometry::EditGeometry {
                rotation: 0.0,
                crop: Some(crate::develop::geometry::CropRect {
                    x: 0.25,
                    y: 0.25,
                    width: 0.5,
                    height: 0.5,
                }),
                crop_ratio: None,
            }),
            ..DevelopStack::default()
        };
        stack.params.insert("exposure".into(), 0.25);
        let thumb = render_file_with_edit(&path, SizeClass::Full, Some(&stack), None)
            .unwrap()
            .expect("真身档");
        assert_eq!((thumb.width, thumb.height), (1200, 900), "半幅裁切 = 原始像素的一半");
    }

    #[test]
    fn render_sig_changes_with_pipeline_version() {
        /*
         * 这条测试的意义：提醒「改算法要改版本号」，否则旧缓存会被当新缓存用。
         *
         * 现在改成**互相校验**：签名串里的版本号必须等于 `PIPELINE_VERSION` ——
         * 只改一处（改常量忘了改字面量，或反过来）就会红，比写死一个数字可靠。
         * （2026-09-16 修「竖拍躺着」时就是这么发现的：改了 `render_file` 的像素输出，
         * 必须同时升这两处，否则旧缓存会被当成新的用。）
         */
        let expected = format!("v{PIPELINE_VERSION}");
        for size in SizeClass::all() {
            assert!(
                render_sig(size).contains(&expected),
                "{size:?} 的签名里要带上 {expected}"
            );
        }
        assert_ne!(render_sig(SizeClass::Grid), render_sig(SizeClass::Screen));
    }

    // ---------- 渲染 ----------

    #[test]
    fn renders_jpeg_and_png_to_an_avif_thumbnail() {
        for bytes in [jpeg_of(4000, 3000, [200, 100, 50]), png_of(300, 200)] {
            let t = render_bytes(&bytes, SizeClass::Grid, None)
                .unwrap()
                .unwrap();
            assert!(is_valid_avif(&t.data), "输出必须是合法 AVIF（缓存格式）");
            assert!(t.width <= GRID_LONG_EDGE && t.height <= GRID_LONG_EDGE);
            assert!(!t.placeholder);
        }
    }

    #[test]
    fn keeps_aspect_ratio_and_fits_the_long_edge() {
        let t = render_bytes(&jpeg_of(4000, 3000, [1, 2, 3]), SizeClass::Grid, None)
            .unwrap()
            .unwrap();
        assert_eq!(t.width, 384, "横图的长边应当是宽度");
        assert_eq!(t.height, 288, "4:3 → 384×288");

        let portrait = render_bytes(&jpeg_of(3000, 4000, [1, 2, 3]), SizeClass::Grid, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            (portrait.width, portrait.height),
            (288, 384),
            "竖图长边是高"
        );

    }

    #[test]
    fn display_aspect_scale_tells_how_much_the_long_edge_shrinks() {
        // 1:10 的图截完只剩 1/3.336 —— 取源要乘这个倍数（崔总 2026-10-10）
        assert!((display_aspect_scale(369, 3693) - 3.336).abs() < 0.01);
        assert!((display_aspect_scale(3693, 369) - 3.336).abs() < 0.01);
        // 比例在范围内（含正好 3:1）不动
        for (w, h) in [(3, 2), (2, 3), (1, 1), (3, 1), (1, 3), (0, 100)] {
            assert_eq!(display_aspect_scale(w, h), 1.0, "{w}×{h} 不该放大取源");
        }
        // 与 clamp_rect 一致：截完的长边 × scale ≈ 原长边
        for (w, h) in [(369, 3693), (5000, 400), (4000, 3000)] {
            let (_, _, kw, kh) = clamp_rect(w, h, MAX_DISPLAY_ASPECT);
            let kept = kw.max(kh) as f32;
            let long = w.max(h) as f32;
            assert!(
                (kept * display_aspect_scale(w, h) - long).abs() <= 2.0,
                "{w}×{h}: 截完 {kept} × scale ≠ {long}"
            );
        }
    }

    #[test]
    fn extreme_aspect_requests_enough_source_before_the_3to1_crop() {
        // Grid（384）：3:1 截取让长边只剩 1/3.336，取源必须补回来，否则截完只有几十像素
        let want = source_input_edge(SizeClass::Grid, (369, 3693), None);
        assert!(want >= 384 * 3, "取源 {want} 太小，截完不够 384");
        assert_eq!(want, 2563, "ceil(384×2×3.336) = 2563");
        // Screen 不做 3:1 夹取 → 只按过采样
        assert_eq!(
            source_input_edge(SizeClass::Screen, (369, 3693), None),
            3840
        );
        // 正常比例不受影响
        assert_eq!(source_input_edge(SizeClass::Grid, (4000, 3000), None), 768);
        // 荒谬比例仍受 8192 上限
        assert_eq!(
            source_input_edge(SizeClass::Grid, (10, 100_000), None),
            8192
        );
    }

    #[test]
    fn extreme_aspect_thumbnail_keeps_the_full_long_edge_after_the_crop() {
        // 先缩后截的旧口径：369×3693 先缩到 384 再截只剩 38×114（糊）。
        // 位图这条路本来就是「整图解出 → 3:1 截 → 缩」，断言它截完仍有满档长边。
        for bytes in [png_of(369, 3693), png_of(3693, 369)] {
            let t = render_bytes(&bytes, SizeClass::Grid, None)
                .unwrap()
                .unwrap();
            assert_eq!(t.width.max(t.height), GRID_LONG_EDGE, "长边必须满档");
            assert_eq!(t.width.min(t.height), 128, "3:1 截取后短边 = 384/3");
            assert!(!t.placeholder);
        }
    }

    #[test]
    fn tight_crop_requests_more_source_pixels_before_final_resize() {
        use crate::develop::geometry::{CropRect, EditGeometry};
        let geometry = EditGeometry {
            rotation: 0.0,
            crop: Some(CropRect {
                x: 0.9,
                y: 0.0,
                width: 0.1,
                height: 1.0,
            }),
            crop_ratio: None,
        };
        assert_eq!(
            geometry_input_edge(SizeClass::Grid, (8000, 1000), None),
            768
        );
        assert_eq!(
            geometry_input_edge(SizeClass::Grid, (8000, 1000), Some(geometry)),
            7680
        );
        assert_eq!(
            geometry_input_edge(SizeClass::Screen, (8000, 1000), Some(geometry)),
            8192
        );
    }

    #[test]
    fn crop_is_applied_before_grid_display_aspect_clamp() {
        use crate::develop::geometry::{CropRect, EditGeometry};
        let stack = DevelopStack {
            geometry: Some(EditGeometry {
                rotation: 0.0,
                crop: Some(CropRect {
                    x: 0.9,
                    y: 0.0,
                    width: 0.1,
                    height: 1.0,
                }),
                crop_ratio: None,
            }),
            ..DevelopStack::default()
        };
        let bytes = png_of(800, 100);
        let thumb = render_bytes_with_edit(&bytes, SizeClass::Grid, None, Some(&stack), None)
            .expect("裁切缩略图")
            .expect("位图");
        assert!(thumb.height > thumb.width, "右侧窄幅裁切应得到竖图");
        assert!(thumb.width <= GRID_LONG_EDGE && thumb.height <= GRID_LONG_EDGE);
    }

    #[test]
    fn resize_for_thumb_keeps_the_size_contract() {
        // 盒式快降只影响画质，不该改变尺寸规则
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(6000, 4000, Rgb([40, 80, 120])));
        let resized = resize_for_thumb(img, GRID_LONG_EDGE);
        assert_eq!(
            (resized.width(), resized.height()),
            (384, 256),
            "6000×4000 → 384×256"
        );
        // 极小的图走直接路径
        let small = DynamicImage::ImageRgb8(RgbImage::from_pixel(800, 600, Rgb([1, 2, 3])));
        assert_eq!(resize_for_thumb(small, GRID_LONG_EDGE).width(), 384);
        // 比目标小：原样
        let tiny = DynamicImage::ImageRgb8(RgbImage::from_pixel(100, 80, Rgb([1, 2, 3])));
        assert_eq!(resize_for_thumb(tiny, GRID_LONG_EDGE).width(), 100);
    }

    #[test]
    fn does_not_upscale_small_images() {
        // 比目标还小的图不该被放大（放大只会更糊、更占空间）
        let t = render_bytes(&jpeg_of(100, 80, [9, 9, 9]), SizeClass::Grid, None)
            .unwrap()
            .unwrap();
        assert_eq!((t.width, t.height), (100, 80));
        // 但也仍然输出合法 JPEG，UI 不用分情况
        assert!(is_valid_avif(&t.data));
    }

    #[test]
    fn non_image_bytes_yield_none_not_error() {
        assert!(
            render_bytes(b"not an image", SizeClass::Grid, None)
                .unwrap()
                .is_none()
        );
        assert!(render_bytes(&[], SizeClass::Grid, None).unwrap().is_none());
        // RAW 文件（这里用假的字节）同样返回 None —— 调用方据此改走占位图
        assert!(
            render_bytes(&[0x49, 0x49, 0x2a, 0x00, 1, 2, 3, 4], SizeClass::Grid, None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn missing_file_is_an_error_not_a_panic() {
        let r = render_file(Path::new("/definitely/not/here.jpg"), SizeClass::Grid);
        assert!(r.is_err(), "文件读不了要报错（调用方好记 missing）");
    }

    // ---------- 朝向 ----------

    #[test]
    fn orientation_rotations_are_applied() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(4, 2, Rgb([1, 2, 3])));
        assert_eq!((img.width(), img.height()), (4, 2));
        // 6 = 顺时针 90°（最常见的「竖着拍」）
        let r6 = apply_orientation(&img, 6);
        assert_eq!((r6.width(), r6.height()), (2, 4));
        // 8 = 逆时针 90°
        let r8 = apply_orientation(&img, 8);
        assert_eq!((r8.width(), r8.height()), (2, 4));
        // 3 = 180°，尺寸不变
        let r3 = apply_orientation(&img, 3);
        assert_eq!((r3.width(), r3.height()), (4, 2));
        // 1 与未知值：原样
        assert_eq!(
            (
                apply_orientation(&img, 1).width(),
                apply_orientation(&img, 1).height()
            ),
            (4, 2)
        );
        assert_eq!(
            (
                apply_orientation(&img, 99).width(),
                apply_orientation(&img, 99).height()
            ),
            (4, 2)
        );
    }

    #[test]
    fn orientation_is_applied_before_resizing() {
        // 竖拍照片（像素 4000×3000 但 orientation=6）：缩略图应当是竖的
        let t = render_bytes(&jpeg_of(4000, 3000, [5, 5, 5]), SizeClass::Grid, Some(6))
            .unwrap()
            .unwrap();
        assert_eq!(
            (t.width, t.height),
            (288, 384),
            "摆正后才缩放，结果应当是竖图"
        );
    }

    #[test]
    fn orientation_flips_do_not_change_size() {
        for o in [2u16, 4, 5, 7] {
            let t = render_bytes(&jpeg_of(400, 300, [1, 1, 1]), SizeClass::Grid, Some(o))
                .unwrap()
                .unwrap();
            assert!(t.width <= GRID_LONG_EDGE);
            assert!(is_valid_avif(&t.data), "orientation={o} 也应当是合法 AVIF");
        }
    }

    // ---------- 占位图 ----------

    #[test]
    fn placeholder_is_a_valid_avif_with_expected_shape() {
        for kind in [MediaKind::Raw, MediaKind::Image, MediaKind::Other] {
            let p = placeholder(kind, SizeClass::Grid).unwrap();
            assert!(p.placeholder, "要标出来是占位图");
            assert!(is_valid_avif(&p.data), "{kind:?} 的占位图必须是合法 AVIF");
            assert_eq!(p.width, GRID_LONG_EDGE);
            assert_eq!(p.height, GRID_LONG_EDGE * 2 / 3);
        }
    }

    #[test]
    fn placeholder_draws_text_pixels() {
        /*
         * 图上应当真的有字（出现前景色像素），而不是一片纯色。
         *
         * 这一条看的是**纯像素那一层**（`placeholder_image`）：纯 Rust 那条路只有
         * AVIF 编码器、没有解码器，编码后再解回来验像素做不到 —— 而「字画没画上去」
         * 是绘制的事，与编码格式无关。
         */
        let img = placeholder_image(MediaKind::Raw, SizeClass::Grid);
        let near_fg = |px: &Rgb<u8>| {
            px.0.iter()
                .zip(FG)
                .map(|(a, b)| a.abs_diff(b) as u32)
                .sum::<u32>()
                < 30
        };
        let fg = img.pixels().filter(|px| near_fg(px)).count();
        assert!(fg > 50, "「RAW」字样该画出可见的像素，实际 {fg}");
    }

    #[test]
    fn glyphs_exist_for_all_placeholder_letters() {
        for c in "RAWI M?".chars().filter(|c| !c.is_whitespace()) {
            assert!(glyph(c).is_some(), "缺字形：{c}");
        }
        assert!(glyph('Z').is_none());
    }

    #[test]
    fn placeholder_encodes_into_the_same_pipeline() {
        // 占位图与原图走同一条编码路径 → 尺寸规则一致，UI 不用分情况
        let p = placeholder(MediaKind::Raw, SizeClass::Grid).unwrap();
        assert!(p.data.len() > 200, "JPEG 不该是空壳");
    }

    #[test]
    fn portrait_photo_is_rotated_upright() {
        /*
         * 竖拍照片在文件里通常是「横着存的 + EXIF 方向 6」——
         * 不摆正就会躺下显示（2026-09-16 人类报的现象）。
         * 这里写一张 80×40 的图 + 方向 6 → 缩略图必须是**竖的**（高 > 宽）。
         */
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("portrait.jpg");
        std::fs::write(
            &path,
            jpeg_with_orientation(&jpeg_of(80, 40, [120, 90, 60]), 6),
        )
        .unwrap();

        let thumb = render_file(&path, SizeClass::Grid).unwrap().unwrap();
        assert!(
            thumb.height > thumb.width,
            "方向 6 的竖拍图应当摆正成竖的，实际 {}×{}",
            thumb.width,
            thumb.height
        );
    }

    #[test]
    fn landscape_photo_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("landscape.jpg");
        std::fs::write(
            &path,
            jpeg_with_orientation(&jpeg_of(80, 40, [30, 60, 90]), 1),
        )
        .unwrap();

        let thumb = render_file(&path, SizeClass::Grid).unwrap().unwrap();
        assert!(
            thumb.width > thumb.height,
            "方向 1 的横拍图不该被转，实际 {}×{}",
            thumb.width,
            thumb.height
        );
    }

    // ---------- 3:1 展示比例夹取（2026-09-16 口径）----------

    #[test]
    fn clamp_rect_centers_and_only_cuts_the_long_side() {
        // 范围内 → 原样
        assert_eq!(clamp_rect(4000, 3000, 3.0), (0, 0, 4000, 3000));
        // 太宽（8:1 → 3:1）：砍宽度、居中
        let (x, y, w, h) = clamp_rect(8000, 1000, 3.0);
        assert_eq!((y, h), (0, 1000));
        assert_eq!(w, 3000);
        assert_eq!(x, (8000 - 3000) / 2, "居中：两边各砍一半");
        // 太高（1:8 → 1:3）：砍高度、居中
        let (x, y, w, h) = clamp_rect(1000, 8000, 3.0);
        assert_eq!((x, w), (0, 1000));
        assert_eq!(h, 3000);
        assert_eq!(y, (8000 - 3000) / 2);
        // 退化输入不炸（0 尺寸 / 非法比例）
        assert_eq!(clamp_rect(0, 100, 3.0), (0, 0, 0, 100));
        assert_eq!(clamp_rect(100, 100, 0.0), (0, 0, 100, 100));
        assert_eq!(clamp_rect(100, 100, f64::NAN), (0, 0, 100, 100));
    }

    #[test]
    fn grid_clamps_a_panorama_but_screen_keeps_it_whole() {
        // 8:1 的全景
        let panorama = jpeg_of(800, 100, [10, 120, 200]);
        let grid = render_bytes(&panorama, SizeClass::Grid, None)
            .unwrap()
            .unwrap();
        let grid_aspect = f64::from(grid.width) / f64::from(grid.height);
        assert!(
            (grid_aspect - 3.0).abs() < 0.05,
            "小图应当夹到 ~3:1，实际 {}×{}（{grid_aspect:.2}）",
            grid.width,
            grid.height
        );

        let screen = render_bytes(&panorama, SizeClass::Screen, None)
            .unwrap()
            .unwrap();
        let screen_aspect = f64::from(screen.width) / f64::from(screen.height);
        assert!(
            (screen_aspect - 8.0).abs() < 0.05,
            "看图档必须保持原始 8:1（全景要看全），实际 {}×{}",
            screen.width,
            screen.height
        );
    }

    #[test]
    fn tall_photos_clamp_the_other_way() {
        let tall = jpeg_of(100, 800, [200, 60, 60]);
        let grid = render_bytes(&tall, SizeClass::Grid, None).unwrap().unwrap();
        let aspect = f64::from(grid.height) / f64::from(grid.width);
        assert!(
            (aspect - 3.0).abs() < 0.05,
            "竖全景应当夹到 1:3，实际 {}×{}",
            grid.width,
            grid.height
        );
    }

    #[test]
    fn normal_aspect_is_untouched_by_the_clamp() {
        let four_thirds = jpeg_of(800, 600, [90, 90, 90]);
        let grid = render_bytes(&four_thirds, SizeClass::Grid, None)
            .unwrap()
            .unwrap();
        let aspect = f64::from(grid.width) / f64::from(grid.height);
        assert!(
            (aspect - 4.0 / 3.0).abs() < 0.02,
            "4:3 在范围内，不该被裁，实际 {aspect:.3}"
        );
    }
}
