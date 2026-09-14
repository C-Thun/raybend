//! 媒体文件与元数据（M1 落地）。
//!
//! 计划承载：文件系统扫描、**文件身份** `(volume_serial, file_id128)`
//! （Windows 用 `GetFileInformationByHandleEx(FileIdInfo)`，可跟踪重命名/移动）、
//! 路径规范化（NFC 存储 + 原始名保留 + 大小写折叠列）、EXIF/IPTC/XMP 抽取。
//! 相关真相与约束见 `AGENTS.md` §7.3。

/// 媒体文件的大类。
///
/// 一个相片仓里可能同时存在 RAW 与普通图像；区分它们决定了后续走哪条
/// 解码 / 缩略图路径（RAW 优先读内嵌预览，普通图像可直接解码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    /// 相机 RAW，需要经 [`crate::raw`] 解码或读取其内嵌预览。
    Raw,
    /// 普通图像（JPEG / PNG / WebP / TIFF / HEIF 等），可直接解码。
    Image,
    /// 其它被索引但不作为照片处理的文件。
    Other,
}

impl MediaKind {
    /// 依扩展名做**初步**判断。
    ///
    /// 真正的判定在 M1 用文件头嗅探取代：扩展名可以撒谎，而这个判断会影响
    /// 走哪条解码路径。RAW 列表与 `FUTURE.md` §B 的首选后端 rawler 支持的格式对齐。
    #[must_use]
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "ari" | "cr3" | "cr2" | "crw" | "erf" | "raf" | "3fr" | "kdc" | "dcs" | "dcr"
            | "iiq" | "mos" | "mef" | "mrw" | "nef" | "nrw" | "orf" | "rw2" | "pef" | "srw"
            | "arw" | "srf" | "sr2" | "dng" => Self::Raw,

            "jpg" | "jpeg" | "png" | "webp" | "tif" | "tiff" | "heif" | "heic" | "avif" => {
                Self::Image
            }

            _ => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MediaKind;

    #[test]
    fn recognises_raw_extensions_case_insensitively() {
        assert_eq!(MediaKind::from_extension("CR3"), MediaKind::Raw);
        assert_eq!(MediaKind::from_extension("nef"), MediaKind::Raw);
        assert_eq!(MediaKind::from_extension("dng"), MediaKind::Raw);
    }

    #[test]
    fn recognises_common_image_extensions() {
        assert_eq!(MediaKind::from_extension("JPG"), MediaKind::Image);
        assert_eq!(MediaKind::from_extension("heic"), MediaKind::Image);
    }

    #[test]
    fn falls_back_to_other() {
        assert_eq!(MediaKind::from_extension("txt"), MediaKind::Other);
        assert_eq!(MediaKind::from_extension(""), MediaKind::Other);
    }
}
