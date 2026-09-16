//! 文件名解析与媒体类型判定。
//!
//! 这里**只按扩展名**做初步判断（快、无需读文件）。真正的类型判定要靠文件头嗅探，
//! 但扩展名判定决定「要不要读这个文件的头」，是扫描阶段的第一道过滤。
//!
//! 规格与出处：
//! * 位图 / RAW 分流 —— `REPOSITORY.md` §4.1（同路径同名的一对文件是一个资产的两个文件）
//! * 相机支持策略 —— `AGENTS.md` §1「跟得上主流即可」
//! * 自家库文件必须跳过 —— `AGENTS.md` §6.4（`catalog.db` 就在被扫描的目录里）

use unicode_normalization::UnicodeNormalization;

/// 媒体文件的大类。
///
/// 一个相片仓里可能同时存在 RAW 与普通图像；区分它们决定了后续走哪条
/// 解码 / 缩略图路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    /// 相机 RAW：本阶段**用占位图**（`FUTURE.md` B8：RAW 后端与进程隔离在后续波次）。
    Raw,
    /// 普通图像（JPEG / PNG / WebP / TIFF / HEIF 等），可直接解码。
    Image,
    /// 其它被索引但不作为照片处理的文件。
    Other,
}

impl MediaKind {
    /// 依扩展名做**初步**判断。传入的扩展名大小写不限，前导点可有可无。
    #[must_use]
    pub fn from_extension(ext: &str) -> Self {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        match ext.as_str() {
            // RAW：与首选后端 rawler 支持的格式对齐（FUTURE.md §B）
            "ari" | "cr3" | "cr2" | "crw" | "erf" | "raf" | "3fr" | "kdc" | "dcs" | "dcr"
            | "iiq" | "mos" | "mef" | "mrw" | "nef" | "nrw" | "orf" | "rw2" | "pef" | "srw"
            | "arw" | "srf" | "sr2" | "dng" => Self::Raw,

            "jpg" | "jpeg" | "png" | "webp" | "tif" | "tiff" | "heif" | "heic" | "avif" => {
                Self::Image
            }

            _ => Self::Other,
        }
    }

    /// 是不是「照片」（RAW 或普通图像）。`Other` 不是。
    #[must_use]
    pub fn is_photo(self) -> bool {
        matches!(self, Self::Raw | Self::Image)
    }
}

/// 取**小写**扩展名（不含点）；没有扩展名时返回 `None`。
///
/// 规则（都是踩过坑的边界）：
/// * **隐藏文件没有扩展名**：`.gitignore` → `None`（而不是 `"gitignore"`）
/// * **末尾点不算扩展名**：`photo.` → `None`
/// * 只取最后一段：`IMG_0001.RW2.xmp` → `"xmp"`，`a.b.c.dng` → `"dng"`
/// * 大小写不敏感：`IMG.JPG` → `"jpg"`
/// * 点开头的多段：`.hidden.tar.gz` → `"gz"`
#[must_use]
pub fn extension(file_name: &str) -> Option<String> {
    // 路径分隔符不该出现在文件名里，但防御一下（有人会把相对路径当名字传进来）
    let name = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let dot = name.rfind('.')?;
    // 点在最前面（隐藏文件）或最后面（末尾点）都不算扩展名
    if dot == 0 || dot + 1 >= name.len() {
        return None;
    }
    Some(name[dot + 1..].to_ascii_lowercase())
}

/// 依文件名判定类型。
#[must_use]
pub fn kind_of_file(file_name: &str) -> MediaKind {
    extension(file_name).map_or(MediaKind::Other, |e| MediaKind::from_extension(&e))
}

/// 是不是侧车文件（伴随主文件的小文件，不属于照片本体）。
///
/// * `.xmp` —— 元数据侧车（`AGENTS.md` §6.4：RAW 永不写回原文件，只写 `.xmp`）
/// * `.aae` —— iOS 编辑记录
/// * `.thm` / `.lrv` —— 相机写的低质量缩略图/低码率代理视频
/// * `.dop` / `.pp3` —— darktable / RawTherapee 的编辑侧车（与本项目无关但会被扫到）
#[must_use]
pub fn is_sidecar(file_name: &str) -> bool {
    matches!(
        extension(file_name).as_deref(),
        Some("xmp" | "aae" | "thm" | "lrv" | "dop" | "pp3")
    )
}

/// 文件名主体（去扩展名）的**折叠形式**：NFC + 小写。
///
/// 用途：把 `IMG_0001.JPG` 与 `img_0001.rw2` 认成同一张照片的两个文件
/// （`REPOSITORY.md` §4.1 的位图 / RAW 配对），以及跨平台比较（Windows 大小写
/// 不敏感、macOS 用 NFD）—— 见 `AGENTS.md` §7.3。
#[must_use]
pub fn stem_folded(file_name: &str) -> String {
    let name = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = match extension(name) {
        Some(ext) => &name[..name.len() - ext.len() - 1],
        None => name,
    };
    stem.nfc().collect::<String>().to_lowercase()
}

/// 明显没有价值的文件属于哪一类（`None` = 不是垃圾，可能是照片）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JunkKind {
    /// 操作系统/文件管理器写的元数据：`Thumbs.db`、`desktop.ini`、`.DS_Store` 等。
    OsMetadata,
    /// NTFS 备用数据流：`photo.jpg:Zone.Identifier`（浏览器下载标记）。
    AlternateDataStream,
    /// 编辑器临时文件：`~$xxx`、`xxx.tmp`、`.goutputstream-xxx`。
    EditorTemp,
    /// 本项目的库文件（`catalog.db` 及其 WAL/SHM）—— **必须跳过**，
    /// 否则会把数据库当照片索引进自己（`AGENTS.md` §6.4）。
    RepositoryFile,
}

/// 判定一个文件是不是「不用管的垃圾」，并说明理由（日志/统计用）。
#[must_use]
pub fn junk_kind(file_name: &str) -> Option<JunkKind> {
    let name = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);

    // NTFS 备用数据流：名字里带 ':'（Windows 上 `photo.jpg:Zone.Identifier`）。
    // 放在最前面判断 —— 这类文件的扩展名是 `Identifier`，看不出问题。
    if name.contains(':') {
        return Some(JunkKind::AlternateDataStream);
    }

    let lower = name.to_lowercase();
    if matches!(
        lower.as_str(),
        "thumbs.db" | "desktop.ini" | ".ds_store" | ".localized" | "ehthumbs.db" | "icon\r"
    ) || lower.starts_with("._")
    {
        // `._xxx` 是 macOS 在非 HFS 卷上写的 AppleDouble 侧车
        return Some(JunkKind::OsMetadata);
    }

    // 系统自己的目录：浏览盘根时一定会撞到，列出来只是噪声
    // （它们可能出现在目录**或**文件位置，按名字判就够了）
    if matches!(
        lower.as_str(),
        "$recycle.bin"
            | "recycler"
            | "system volume information"
            | "$windows.~ws"
            | "$windows.~bt"
            | ".trash"
            | ".trashes"
            | ".spotlight-v100"
            | ".fseventsd"
            | ".documentrevisions-v100"
    ) {
        return Some(JunkKind::OsMetadata);
    }

    if lower.starts_with("~$") || lower.ends_with(".tmp") || lower.starts_with(".goutputstream-") {
        return Some(JunkKind::EditorTemp);
    }

    // 自家库文件：catalog.db / catalog.db-wal / catalog.db-shm（含 WAL 与 SHM 边车）
    let base = crate::store::repository::CATALOG_FILE_NAME;
    if lower == base || lower.starts_with(&format!("{base}-")) {
        return Some(JunkKind::RepositoryFile);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- 扩展名解析 ----------

    #[test]
    fn extension_is_lowercased_and_without_dot() {
        assert_eq!(extension("IMG_0001.JPG").as_deref(), Some("jpg"));
        assert_eq!(extension("a.NeF").as_deref(), Some("nef"));
        assert_eq!(extension("photo").as_deref(), None);
    }

    #[test]
    fn hidden_files_have_no_extension() {
        // 这是最容易写错的一条：`.gitignore` 的「扩展名」不是 gitignore
        assert_eq!(extension(".gitignore"), None);
        assert_eq!(extension(".jpg"), None, "点开头的整名都是隐藏文件");
        assert_eq!(kind_of_file(".gitignore"), MediaKind::Other);
    }

    #[test]
    fn trailing_dot_is_not_an_extension() {
        assert_eq!(extension("photo."), None);
        assert_eq!(extension("photo.."), None);
        assert_eq!(kind_of_file("photo."), MediaKind::Other);
    }

    #[test]
    fn takes_only_the_last_segment() {
        assert_eq!(extension("a.b.c.dng").as_deref(), Some("dng"));
        assert_eq!(extension("IMG.RW2.xmp").as_deref(), Some("xmp"));
        assert_eq!(extension(".hidden.tar.gz").as_deref(), Some("gz"));
    }

    #[test]
    fn handles_unicode_and_long_names() {
        let long = "实".repeat(200) + ".JPG";
        assert_eq!(extension(&long).as_deref(), Some("jpg"));
        assert_eq!(kind_of_file("照片-2026年9月.JPG"), MediaKind::Image);
        assert_eq!(
            kind_of_file("写真.ＲＡＷ"),
            MediaKind::Other,
            "全角不等于半角"
        );
        assert_eq!(extension("").as_deref(), None);
        assert_eq!(extension(".").as_deref(), None);
    }

    #[test]
    fn tolerates_accidental_paths() {
        // 有人把相对/绝对路径当文件名传进来时不该出错
        assert_eq!(extension("dir/sub/IMG.JPG").as_deref(), Some("jpg"));
        assert_eq!(extension(r"C:\photos\IMG.RW2").as_deref(), Some("rw2"));
        assert_eq!(stem_folded("dir/sub/IMG.JPG"), "img");
    }

    // ---------- 类型判定 ----------

    #[test]
    fn recognises_raw_and_image_case_insensitively() {
        assert_eq!(MediaKind::from_extension("CR3"), MediaKind::Raw);
        assert_eq!(MediaKind::from_extension(".nef"), MediaKind::Raw);
        assert_eq!(MediaKind::from_extension("rw2"), MediaKind::Raw);
        assert_eq!(MediaKind::from_extension("JPG"), MediaKind::Image);
        assert_eq!(MediaKind::from_extension("heic"), MediaKind::Image);
        assert_eq!(MediaKind::from_extension(""), MediaKind::Other);
        assert_eq!(MediaKind::from_extension("txt"), MediaKind::Other);
    }

    #[test]
    fn is_photo_covers_raw_and_image_only() {
        assert!(MediaKind::Raw.is_photo());
        assert!(MediaKind::Image.is_photo());
        assert!(!MediaKind::Other.is_photo());
    }

    // ---------- 侧车 ----------

    #[test]
    fn sidecars_are_recognised() {
        for name in [
            "IMG.RW2.xmp",
            "clip.aae",
            "MVI_0001.THM",
            "a.LRV",
            "x.dop",
            "y.pp3",
        ] {
            assert!(is_sidecar(name), "{name} 应判为侧车");
        }
        for name in ["IMG.JPG", "IMG.RW2", "notes.md", ".xmp"] {
            assert!(!is_sidecar(name), "{name} 不该判为侧车");
        }
    }

    // ---------- 配对用的 stem ----------

    #[test]
    fn stem_folded_pairs_bitmap_with_raw() {
        assert_eq!(stem_folded("IMG_0001.JPG"), "img_0001");
        assert_eq!(stem_folded("img_0001.rw2"), "img_0001");
        assert_eq!(stem_folded("IMG_0001.JPG"), stem_folded("IMG_0001.RW2"));
        assert_eq!(stem_folded("noext"), "noext");
        // NFC/NFD 变体折叠后相等（跨平台配对的前提）
        assert_eq!(stem_folded("cafe\u{301}.jpg"), stem_folded("café.jpg"));
    }

    // ---------- 垃圾文件 ----------

    #[test]
    fn os_metadata_is_junk() {
        for name in [
            "Thumbs.db",
            "thumbs.DB",
            "desktop.ini",
            ".DS_Store",
            "._IMG_0001.jpg",
        ] {
            assert!(
                matches!(junk_kind(name), Some(JunkKind::OsMetadata)),
                "{name}"
            );
        }
    }

    #[test]
    fn alternate_data_streams_are_junk() {
        assert_eq!(
            junk_kind("IMG_0001.jpg:Zone.Identifier"),
            Some(JunkKind::AlternateDataStream)
        );
        assert_eq!(
            junk_kind("photo:stream"),
            Some(JunkKind::AlternateDataStream)
        );
    }

    #[test]
    fn editor_temp_files_are_junk() {
        assert_eq!(junk_kind("~$document.docx"), Some(JunkKind::EditorTemp));
        assert_eq!(junk_kind("scan.tmp"), Some(JunkKind::EditorTemp));
        assert_eq!(
            junk_kind(".goutputstream-ABC123"),
            Some(JunkKind::EditorTemp)
        );
    }

    #[test]
    fn our_own_database_is_junk() {
        assert_eq!(
            junk_kind("catalog.db"),
            Some(JunkKind::RepositoryFile),
            "库文件绝不能被当成照片扫进来"
        );
        assert_eq!(junk_kind("catalog.db-wal"), Some(JunkKind::RepositoryFile));
        assert_eq!(junk_kind("CATALOG.DB-SHM"), Some(JunkKind::RepositoryFile));
    }

    #[test]
    fn system_directories_are_junk() {
        // 浏览盘根（`D:\`）时一定会碰到它们 —— 列进来源树或网格只是噪声
        for name in [
            "$RECYCLE.BIN",
            "$Recycle.Bin",
            "RECYCLER",
            "System Volume Information",
            "$WINDOWS.~WS",
            ".Trash",
            ".Spotlight-V100",
        ] {
            assert_eq!(
                junk_kind(name),
                Some(JunkKind::OsMetadata),
                "{name} 是系统自己的目录"
            );
        }
        // 只是名字里带一点相像的，不能误伤
        assert_eq!(junk_kind("Recycle Bin Photos"), None);
        assert_eq!(junk_kind("trash"), None);
    }

    #[test]
    fn ordinary_photos_are_not_junk() {
        for name in [
            "IMG_0001.JPG",
            "IMG_0001.RW2",
            "照片.png",
            "a.xmp",
            "notes.txt",
        ] {
            assert_eq!(junk_kind(name), None, "{name} 不是垃圾（由调用方按需过滤）");
        }
    }
}

/// 这个条目算「隐藏」吗？—— 目录树与照片列表默认都跳过（人类 2026-09-16：
/// 「目录查看器里不显示隐藏目录」）。
///
/// 两条判据：
/// 1. **名字以 `.` 开头**（跨平台都常见：`.git`、`.thumbnails`、`.DS_Store`）；
/// 2. **Windows 的隐藏属性**（`FILE_ATTRIBUTE_HIDDEN`）——
///    在 Windows 上「隐藏」多半是靠属性而不是点前缀，只看名字会漏。
///
/// 没有「显示隐藏项」的开关：需要时再加（`ScanOptions::include_hidden` 管的是照片扫描那一侧）。
#[must_use]
pub fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.')
}

/// Windows 隐藏属性；其它平台恒为 `false`（那边靠点前缀）。
#[must_use]
pub fn has_hidden_attribute(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

#[cfg(test)]
mod hidden_tests {
    use super::{has_hidden_attribute, is_hidden_name};

    #[test]
    fn 点开头的算隐藏() {
        assert!(is_hidden_name(".git"));
        assert!(is_hidden_name(".DS_Store"));
        assert!(!is_hidden_name("photos"));
        assert!(!is_hidden_name("我的照片"));
        // 点不在开头不算（`..` 之类本来也不会出现在 read_dir 里）
        assert!(!is_hidden_name("a.b"));
    }

    #[test]
    fn 普通文件没有隐藏属性() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, b"x").unwrap();
        let metadata = std::fs::metadata(&file).unwrap();
        // Windows 上新建的文件默认没有隐藏属性；其它平台恒 false
        assert!(!has_hidden_attribute(&metadata));
    }
}
