//! **LUT**（查找表）—— `.cube` 与 HaldCLUT 的解析、求值与封面烘焙。
//!
//! > **现状：落点占位。** 这里目前只有内置样片资源；解析 / 求值 / 封面烘焙
//! > 随 `plans/M3-W6c.md` 落地。规格见 `PLAN.md` M3-W6c 与 `design/editor.md` §3.1.1。
//!
//! # 兼容范围（人类 2026-09-25 定：**只两种**）
//!
//! * **`.cube`**（1D / 3D / shaper）—— 要处理对四件事：
//!   1. **Red 变化最快**的行序（Adobe 规范：等价 C 索引 `r + N*g + N*N*b`）——
//!      这是最容易踩反的一条；
//!   2. `DOMAIN_MIN` / `DOMAIN_MAX` —— 输入域不一定是 `[0, 1]`；
//!   3. **1D shaper + 3D 的组合**（1D 先应用，不是所有实现都处理）；
//!   4. 尺寸不定：17³ / 33³ / 65³ 都要能读（33³ 是创意 LUT 的常见档）。
//! * **HaldCLUT**（PNG / TIFF）—— 与 `.cube` 共用同一条**三线性插值 + `strength` 混合**，
//!   只换「从图里取表」的 loader。规格（已核实 `rtengine/clutstore.cc::loadFile`）：
//!   **图边长 = level³、表边长 = level²**，即 512×512 = level 8 = 64³ 表。
//!
//! **明确不做**：`.3dl` / `.look` / `.csp` / `.cub` / `.itx` / `.mga` / `.spi*` / `.blut`
//! 等影视私有格式；`.icc` / `.icm` 里的 LUT 归色彩管理（`FUTURE.md` C1），**不放 LUT 面板**。

/// 内置样片：**768×576、4:3、有损 WebP**，约 84 KB。
///
/// 用途：给**没有自带演示图**的 LUT 生成封面 —— 用它套上该 LUT 再烘焙
/// （`design/editor.md` §3.1.1 的「回落」那一档）。
///
/// # 为什么编译期嵌入
///
/// 它只读、很小，且必须在**任何路径下**都可用（封面烘焙发生在导入时，那时可能连库都还没打开）。
/// 走 `include_bytes!` 比走 Tauri 的资源 API 简单，也不把 `crates/raybend` 与 Tauri 绑在一起
/// （`AGENTS.md` §6.2 的分层纪律）。本仓此前没有 `include_bytes!` 先例，这是第一处。
///
/// # 这张图的来源与处理（人类 2026-09-25 提供）
///
/// 给的原件带一个 **41,974 字节的 EXIF chunk**（占全文 32%），里面是
/// `OLYMPUS E-M5MarkII` / `LEICA DG 12-60/F2.8-4.0` / 拍摄时间 `2024:07:09 14:39:16`。
/// 入库前做了一次**纯字节级的 chunk 剥离**（RIFF 容器直接删掉，**不重编码、零质量损失**）：
/// 127,506 → 85,524 字节。
///
/// **别把带元数据的版本重新放进来** —— 拍摄隐私不该进安装包，下面有测试守着。
pub const SAMPLE_WEBP: &[u8] = include_bytes!("../../assets/lut-sample.webp");

/// 样片画布尺寸 —— `design/editor.md` §3.1.1 的「大封面」档就是照它定的。
pub const SAMPLE_WIDTH: u32 = 768;
/// 样片画布尺寸（见 [`SAMPLE_WIDTH`]）。
pub const SAMPLE_HEIGHT: u32 = 576;

#[cfg(test)]
mod tests {
    use super::*;

    /// 遍历 RIFF chunk 的 fourcc。
    ///
    /// **不能**用「在字节流里搜 `EXIF` 四个字母」代替：图像数据里碰巧出现这四个字节
    /// 是低概率但非零的事件，那就是个会偶发变红的假测试。
    fn riff_chunks(data: &[u8]) -> Vec<[u8; 4]> {
        let mut out = Vec::new();
        let mut offset = 12; // 跳过 `RIFF <size> WEBP`
        while offset + 8 <= data.len() {
            let fourcc = data[offset..offset + 4].try_into().expect("4 字节");
            let size = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().expect("4 字节"));
            out.push(fourcc);
            offset += 8 + size as usize + (size as usize & 1); // chunk 按偶数字节对齐
        }
        out
    }

    #[test]
    fn sample_is_a_decodable_webp_of_the_declared_size() {
        let image = image::load_from_memory_with_format(SAMPLE_WEBP, image::ImageFormat::WebP)
            .expect("内置样片必须能解码");
        assert_eq!((image.width(), image.height()), (SAMPLE_WIDTH, SAMPLE_HEIGHT));
        // 4:3 —— 封面规格（`design/editor.md` §3.1.1）建立在这上面
        assert_eq!(SAMPLE_WIDTH * 3, SAMPLE_HEIGHT * 4);
    }

    #[test]
    fn sample_carries_no_metadata_chunks() {
        // 回归：人类提供的原件带 42 KB 的 EXIF（相机 / 镜头 / 拍摄时间）。
        // 那是拍摄隐私，还会白占 32% 体积 —— 不许再回来。
        assert_eq!(&SAMPLE_WEBP[..4], b"RIFF");
        assert_eq!(&SAMPLE_WEBP[8..12], b"WEBP");
        let chunks = riff_chunks(SAMPLE_WEBP);
        assert!(chunks.contains(b"VP8 "), "应当是图像 chunk，实际 {chunks:?}");
        for banned in [b"EXIF", b"XMP "] {
            assert!(
                !chunks.contains(banned),
                "内置样片不许带 {} chunk，实际 {chunks:?}",
                String::from_utf8_lossy(banned)
            );
        }
    }
}
