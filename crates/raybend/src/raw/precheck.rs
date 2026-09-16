//! 进 worker 之前的**廉价预检**：文件大小 + 文件头魔数。
//!
//! 为什么要有它（`AGENTS.md` §6.3）：上游明确声明「不要把 dnglab/rawler 用于处理
//! **不可信**文件」。真正的隔离靠 worker 进程，但预检能挡掉三类东西：
//!
//! 1. **不是 RAW 的文件**（把 3GB 的视频改名成 `.rw2` 之类）——不必浪费一次进程与解码；
//! 2. **荒谬的大小**（< 1KB 连一个 IFD 都装不下；> 512MB 现实中没有）；
//! 3. **读不到的文件**（权限 / 被占用 / 路径是目录）。
//!
//! 预检**不做**深度校验（那是解码器的事），也**不保证安全** —— 它只是把明显不对的
//! 挡在进程边界之外（构造一个能通过魔数预检的恶意文件依然轻而易举）。

use std::io::Read;
use std::path::Path;

/// 小于这个字节数不可能是 RAW（连一个 TIFF 头 + IFD 都不够）。
pub const MIN_BYTES: u64 = 1024;
/// 大于这个字节数直接拒绝（现实中的 RAW 都在 200MB 以下）。
pub const MAX_BYTES: u64 = 512 * 1024 * 1024;
/// 嗅探需要读的前导字节数。
pub const HEAD_BYTES: usize = 32;

/// 容器家族（只看文件头就能确定的那些）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// 标准 TIFF 结构（`II\x2a\x00` / `MM\x00\x2a`）：
    /// NEF / ARW / DNG / CR2 / PEF / SRW / NRW / KDC / DCR / MEF / ERF / MOS / 3FR…
    Tiff,
    /// Panasonic RW2 —— **也是 TIFF 结构，但魔数是 0x55 不是 0x2A**（实测踩过：
    /// 只认 0x2A 会把全部 RW2 挡在门外）。
    Rw2,
    /// Olympus / OM System ORF —— TIFF 结构，魔数 `IIRO` / `MMOR`。
    Orf,
    /// Canon CR3（ISO-BMFF）。
    Cr3,
    /// Fujifilm RAF。
    Raf,
    /// Phase One IIQ。
    Iiq,
    /// Sigma X3F。
    X3f,
    /// Minolta MRW。
    Mrw,
    /// Canon CRW（老 CIFF）。
    Crw,
}

impl Container {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tiff => "tiff",
            Self::Rw2 => "rw2",
            Self::Orf => "orf",
            Self::Cr3 => "cr3",
            Self::Raf => "raf",
            Self::Iiq => "iiq",
            Self::X3f => "x3f",
            Self::Mrw => "mrw",
            Self::Crw => "crw",
        }
    }
}

/// 预检结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Precheck {
    /// 可以进 worker。
    Ok { container: Container, bytes: u64 },
    /// 不进 worker，附人话解释（会进日志，不会弹给用户）。
    Rejected(String),
}

impl Precheck {
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok { .. })
    }
}

/// 只按文件头判断容器。`head` 是文件前若干字节（至少 12 个才有意义）。
#[must_use]
pub fn sniff(head: &[u8]) -> Option<Container> {
    // ── 先判「有明确专属魔数」的，避免被下面的通用 TIFF 吃掉 ──
    if head.starts_with(b"FUJIFILMCCD-RAW") {
        return Some(Container::Raf);
    }
    if head.starts_with(b"FOVb") {
        return Some(Container::X3f);
    }
    if head.starts_with(b"IIII") {
        return Some(Container::Iiq);
    }
    if head.starts_with(b"\x00MRM") {
        return Some(Container::Mrw);
    }
    // Canon CRW：`II` + 0x1a 0x00 + 0x00 0x00 + "HEAPCCDR"
    if head.starts_with(b"II\x1a\x00\x00\x00HEAPCCDR") {
        return Some(Container::Crw);
    }
    // CR3：ISO-BMFF，`ftyp` 在偏移 4，主品牌是 `crx `
    if head.len() >= 12 && &head[4..8] == b"ftyp" {
        let brand = &head[8..12];
        if brand == b"crx " || brand == b"crx\0" {
            return Some(Container::Cr3);
        }
        // 其它 ftyp 不是我们管的（mp4/heic…）
        return None;
    }
    // ── 最后才是 TIFF 家族（几个厂商用不同的魔数，都要认）──
    // 标准 TIFF：0x2A
    if head.starts_with(b"II\x2a\x00") || head.starts_with(b"MM\x00\x2a") {
        return Some(Container::Tiff);
    }
    // Panasonic RW2：0x55（实测：本项目的 RW2 样本全是这个头）
    if head.starts_with(b"IIU\x00") || head.starts_with(b"MM\x00U") {
        return Some(Container::Rw2);
    }
    // Olympus ORF：`IIRO` / `MMOR`
    if head.starts_with(b"IIRO") || head.starts_with(b"MMOR") {
        return Some(Container::Orf);
    }
    None
}

/// 对一个路径做预检（stat + 读前 [`HEAD_BYTES`] 字节）。
#[must_use]
pub fn precheck(path: &Path) -> Precheck {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return Precheck::Rejected(format!("读不到文件信息：{e}")),
    };
    if !meta.is_file() {
        return Precheck::Rejected("不是普通文件".to_string());
    }
    let bytes = meta.len();
    if bytes < MIN_BYTES {
        return Precheck::Rejected(format!("文件太小（{bytes} 字节）"));
    }
    if bytes > MAX_BYTES {
        return Precheck::Rejected(format!("文件太大（{bytes} 字节）"));
    }
    let mut buf = [0u8; HEAD_BYTES];
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return Precheck::Rejected(format!("打不开文件：{e}")),
    };
    // 读不满不算错（文件可能恰好很短，但已通过 MIN_BYTES），读多少判多少
    let read = match file.read(&mut buf) {
        Ok(n) => n,
        Err(e) => return Precheck::Rejected(format!("读文件头失败：{e}")),
    };
    match sniff(&buf[..read]) {
        Some(container) => Precheck::Ok { container, bytes },
        None => Precheck::Rejected("文件头不是已知的 RAW 容器".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_tiff_both_endians() {
        assert_eq!(sniff(b"II\x2a\x00rest"), Some(Container::Tiff));
        assert_eq!(sniff(b"MM\x00\x2arest"), Some(Container::Tiff));
    }

    #[test]
    fn sniffs_panasonic_rw2_and_olympus_orf() {
        // 真实样本的第一手事实（`/mnt/c/src/tmp/pic/*.RW2`）：
        // `49 49 55 00` = “IIU\0” —— 魔数是 0x55，不是标准 TIFF 的 0x2A。
        // 只认 0x2A 的话**全部 RW2 都会被挡在门外**（这就是当时冒烟报的错）。
        assert_eq!(
            sniff(&[0x49, 0x49, 0x55, 0x00, 0x18, 0x00]),
            Some(Container::Rw2)
        );
        assert_eq!(sniff(b"MM\x00U"), Some(Container::Rw2), "大端变体也要认");
        assert_eq!(sniff(b"IIRO\x08\x00"), Some(Container::Orf));
        assert_eq!(sniff(b"MMOR"), Some(Container::Orf));
    }

    #[test]
    fn sniffs_specific_containers_before_tiff_fallback() {
        assert_eq!(sniff(b"FUJIFILMCCD-RAW 0201"), Some(Container::Raf));
        assert_eq!(sniff(b"FOVb\x00\x00"), Some(Container::X3f));
        assert_eq!(sniff(b"IIII\x00\x00\x00\x00"), Some(Container::Iiq));
        assert_eq!(sniff(b"\x00MRMxxxx"), Some(Container::Mrw));
        assert_eq!(
            sniff(b"II\x1a\x00\x00\x00HEAPCCDR"),
            Some(Container::Crw),
            "CRW 的魔数比通用 TIFF 更具体，必须先判"
        );
    }

    #[test]
    fn sniffs_cr3_but_not_other_iso_bmff() {
        let mut head = vec![0u8; 4];
        head.extend_from_slice(b"ftyp");
        head.extend_from_slice(b"crx ");
        assert_eq!(sniff(&head), Some(Container::Cr3));

        let mut heic = vec![0u8; 4];
        heic.extend_from_slice(b"ftyp");
        heic.extend_from_slice(b"heic");
        assert_eq!(sniff(&heic), None, "HEIC 不是我们的 RAW");
    }

    #[test]
    fn rejects_junk_headers() {
        assert_eq!(sniff(b"\xff\xd8\xff\xe0jpeg"), None, "JPEG 不是 RAW");
        assert_eq!(sniff(b"PK\x03\x04zip"), None);
        assert_eq!(sniff(b""), None);
        assert_eq!(sniff(b"II\x2a"), None, "头太短不能瞎认");
    }

    #[test]
    fn precheck_rejects_missing_file_and_directory() {
        let missing = Path::new("/definitely/not/here/IMG.RW2");
        assert!(!precheck(missing).is_ok());

        let dir = tempfile::tempdir().unwrap();
        match precheck(dir.path()) {
            Precheck::Rejected(msg) => assert!(msg.contains("不是普通文件")),
            other => panic!("目录不该通过预检：{other:?}"),
        }
    }

    #[test]
    fn precheck_rejects_tiny_and_huge_files() {
        let dir = tempfile::tempdir().unwrap();

        let tiny = dir.path().join("tiny.rw2");
        std::fs::write(&tiny, b"II\x2a\x00").unwrap();
        match precheck(&tiny) {
            Precheck::Rejected(msg) => assert!(msg.contains("太小"), "{msg}"),
            other => panic!("{other:?}"),
        }

        // 「太大」用稀疏文件构造，不真占磁盘
        let huge = dir.path().join("huge.rw2");
        let file = std::fs::File::create(&huge).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        drop(file);
        match precheck(&huge) {
            Precheck::Rejected(msg) => assert!(msg.contains("太大"), "{msg}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn precheck_accepts_a_plausible_tiff_raw() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("IMG.RW2");
        // 写成真实的 RW2 头（`IIU\0`），而不是标准 TIFF —— 这条用例要盯住的
        // 就是「RW2 必须能过预检」
        let mut bytes = b"IIU\x00".to_vec();
        bytes.resize(4096, 0);
        std::fs::write(&path, &bytes).unwrap();

        match precheck(&path) {
            Precheck::Ok { container, bytes } => {
                assert_eq!(container, Container::Rw2);
                assert_eq!(bytes, 4096);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn container_names_are_stable() {
        // 会进日志与诊断报告
        assert_eq!(Container::Tiff.as_str(), "tiff");
        assert_eq!(Container::Rw2.as_str(), "rw2");
        assert_eq!(Container::Orf.as_str(), "orf");
        assert_eq!(Container::Cr3.as_str(), "cr3");
    }
}
