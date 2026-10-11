//! 位图 / RAW 的**配对继承**：RAW 自己没读到拍摄时间时，从同名的位图那里继承。
//!
//! ## 为什么需要它
//!
//! 「同一张照片的 JPG 与 RAW 是同一时刻」这件事是确定的（`memory/FUNCTION-REPOSITORY.md` §4.1 的配对规则
//! 就是照这个来的）。但**相机不一定给 RAW 写 `DateTimeOriginal`** —— 这时 RAW 会退到
//! 文件名 / mtime 兜底，而 mtime 是**拷贝时间**，常常和拍摄时间差几小时。
//!
//! 后果不是「时间不准」这么轻：按时间分组时，同一张照片的两个文件会落进**不同的时间片**，
//! 界面上就是「同一天的分组标题出现两次、一次纯位图一次纯 RAW」（人类 2026-09-17/18 两次上报）。
//!
//! ## 规则
//!
//! * 配对键 = **目录**（折叠：小写、分隔符归一、去掉结尾斜杠）+ **主名**（`kind::stem_folded`，
//!   即 NFC + 小写 + 去扩展名）；
//! * 目录末尾若是 `_RAW` 就**去掉那一级** —— 库里 RAW 落在同级的 `_RAW/` 下
//!   （`memory/FUNCTION-REPOSITORY.md` §4.1），去掉之后与位图同键；
//! * 只有 `is_raw && taken_at.is_none()` 的条目会被填；
//! * 同一键位上有多张位图时取**最早**的那个时间（顺序确定，不依赖遍历顺序）；
//! * 同键没有位图、或位图自己也没时间 → 不动。
//!
//! 继承来的时间**整份**来自那张位图（连来源标记一起）—— 它本来就是「这张照片的时间」，
//! 只是这次是从姊妹文件读到的；调用方要区分的话看得到 `is_raw`。

use std::collections::HashMap;
use std::path::Path;

use crate::media::exif::TakenAtSource;
use crate::media::kind::{self, MediaKind};

/// 配对继承的一条输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimePair {
    /// 配对键（见模块文档的「规则」）。
    pub key: String,
    /// 这条是不是 RAW。
    pub is_raw: bool,
    /// 已读到的拍摄时间（Unix 毫秒）；`None` = 没读到。
    pub taken_at: Option<i64>,
}

impl TimePair {
    /// 由路径、是否 RAW、以及已读到的时间造一条。
    #[must_use]
    pub fn new(path: &Path, is_raw: bool, taken_at: Option<i64>) -> Self {
        Self {
            key: pair_key(path),
            is_raw,
            taken_at,
        }
    }
}

/// 配对键：`<折叠后的目录>/<折叠后的主名>`（目录为空时只有主名）。
///
/// ⚠️ **两种分隔符都自己拆**，不用 `Path::parent()`：应用跑在 Windows（`\`），
/// 但库里存的是 `/`，而 `Path` 在 Linux/macOS 上根本不把 `\` 当分隔符 ——
/// 靠平台语义就会出现「同一张照片在两边不同键」这种灵异现象（实测：一个混用两种
/// 分隔符的用例里，`C:\Photos\x.JPG` 被判成了「没有目录」）。
#[must_use]
pub fn pair_key(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let trimmed = raw.trim_end_matches('/');
    let (dir_raw, name) = match trimmed.rfind('/') {
        Some(at) => (&trimmed[..at], &trimmed[at + 1..]),
        None => ("", trimmed),
    };

    let stem = kind::stem_folded(name);
    let mut dir = dir_raw.trim_end_matches('/').to_lowercase();
    // 库里的 RAW 在 `<同级>/_RAW/` 下 —— 去掉这一级才与位图同键
    if let Some(stripped) = dir.strip_suffix("/_raw") {
        dir = stripped.to_string();
    }
    if dir.is_empty() { stem } else { format!("{dir}/{stem}") }
}

/// 让「没读到时间的 RAW」继承同键位图的时间，返回继承了几条。
pub fn inherit_times_from_bitmaps(pairs: &mut [TimePair]) -> usize {
    // 先找出每个键上「位图那一侧最早的时间」（own 的 key，免得与下面的可变借用打架）
    let mut donor: HashMap<String, i64> = HashMap::new();
    for pair in pairs.iter() {
        if pair.is_raw {
            continue;
        }
        let Some(millis) = pair.taken_at else {
            continue;
        };
        donor
            .entry(pair.key.clone())
            .and_modify(|current| {
                if millis < *current {
                    *current = millis;
                }
            })
            .or_insert(millis);
    }

    let mut filled = 0;
    for pair in pairs.iter_mut() {
        if !pair.is_raw || pair.taken_at.is_some() {
            continue;
        }
        if let Some(millis) = donor.get(&pair.key) {
            pair.taken_at = Some(*millis);
            filled += 1;
        }
    }
    filled
}

/// 把配对继承**直接作用在一批元信息上**（`dir_meta_ensure` 的用法）。
///
/// `names[i]` 是第 i 条的文件名（**目录内相对名**，不是绝对路径 —— 配对键只看主名，
/// 同一批本来就在同一个目录里）。返回被填上的条数。
///
/// 与 [`inherit_times_from_bitmaps`] 的关系：规则只有这一套（谁借谁、借哪一个），
/// 这里只是把「读头读出来的元信息」搬进 `TimePair` 再搬回去 —— 调用方不必自己写第二遍。
/// 借来的时间**来源写成 [`TakenAtSource::Sibling`]**（界面上「这条时间不是从它自己读到的」
/// 是有意义的），而原来就有时间的（哪怕只是 mtime 兜底）不动。
pub fn inherit_times_in_metas(names: &[String], metas: &mut [crate::media::meta::PhotoMeta]) -> usize {
    if names.len() != metas.len() || names.is_empty() {
        return 0;
    }
    let mut pairs: Vec<TimePair> = names
        .iter()
        .map(|name| {
            TimePair::new(
                Path::new(name),
                kind::kind_of_file(name) == MediaKind::Raw,
                None,
            )
        })
        .collect();
    /*
     * 只把**位图那些**的时间喂进配对表：RAW 自己读到的时间不该参与「借出」，
     * 而且 `inherit_times_from_bitmaps` 本来就只从位图取 donor —— 所以这里把
     * 每条的时间按「是不是 RAW」分开喂：位图给真值，RAW 一律给 `None`（空位=vacancy）。
     */
    for (pair, meta) in pairs.iter_mut().zip(metas.iter()) {
        if !pair.is_raw {
            pair.taken_at = meta.taken_at_ms;
        }
    }
    if inherit_times_from_bitmaps(&mut pairs) == 0 {
        return 0;
    }
    let mut filled = 0;
    for ((pair, meta), name) in pairs.iter().zip(metas.iter_mut()).zip(names.iter()) {
        let _ = name;
        if !pair.is_raw || meta.taken_at_ms.is_some() {
            continue;
        }
        let Some(millis) = pair.taken_at else { continue };
        meta.taken_at_ms = Some(millis);
        meta.taken_at_source = Some(TakenAtSource::Sibling);
        filled += 1;
    }
    filled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 元信息上做配对继承_raw借位图的时间() {
        use crate::media::meta::PhotoMeta;

        let names = vec![
            "P1000019.JPG".to_string(),
            "P1000019.RW2".to_string(),
            "P1000020.RW2".to_string(),
        ];
        let mut metas = vec![
            PhotoMeta {
                taken_at_ms: Some(1000),
                taken_at_source: Some(TakenAtSource::Exif),
                ..PhotoMeta::default()
            },
            // 这张 RAW 自己没读到时间 → 借上面那张 JPG 的
            PhotoMeta::default(),
            // 这张有自己读到的（没有位图兄弟）→ 一个字节都不动
            PhotoMeta {
                taken_at_ms: Some(2000),
                taken_at_source: Some(TakenAtSource::Exif),
                ..PhotoMeta::default()
            },
        ];

        assert_eq!(inherit_times_in_metas(&names, &mut metas), 1);
        assert_eq!(metas[1].taken_at_ms, Some(1000), "RAW 借到同名位图的时间");
        assert_eq!(
            metas[1].taken_at_source,
            Some(TakenAtSource::Sibling),
            "借来的要标成 Sibling（不是它自己读到的）"
        );
        assert_eq!(metas[2].taken_at_ms, Some(2000));
        assert_eq!(metas[2].taken_at_source, Some(TakenAtSource::Exif));

        // 长度对不上就当没这回事（调用方自己会核对）——不 panic
        assert_eq!(inherit_times_in_metas(&names, &mut metas[..1]), 0);
    }

    fn pairs(list: &[(&str, bool, Option<i64>)]) -> Vec<TimePair> {
        list.iter()
            .map(|(path, is_raw, ms)| TimePair::new(Path::new(path), *is_raw, *ms))
            .collect()
    }

    #[test]
    fn raw_inherits_from_same_stem_bitmap() {
        // RAW 没有时间（或只有 mtime 兜底 → 上游会给 None 或一个拷贝时间，这里按 None 走），
        // 位图有——RAW 应当继承过去
        let mut list = pairs(&[
            ("/src/P1000019.JPG", false, Some(1000)),
            ("/src/P1000019.RW2", true, None),
            ("/src/P1000020.JPG", false, Some(2000)),
            ("/src/P1000020.RW2", true, None),
        ]);
        assert_eq!(inherit_times_from_bitmaps(&mut list), 2);
        assert_eq!(list[1].taken_at, Some(1000));
        assert_eq!(list[3].taken_at, Some(2000));
    }

    #[test]
    fn extension_case_and_separators_do_not_matter() {
        // 大小写不同、扩展名不同、Windows 分隔符 —— 都该认成一对（AGENTS.md §7.3）
        let mut list = pairs(&[
            (r"C:\Photos\IMG_0001.JPG", false, Some(500)),
            (r"C:\Photos\img_0001.RW2", true, None),
            ("c:/photos/IMG_0001.ORF", true, None),
        ]);
        assert_eq!(inherit_times_from_bitmaps(&mut list), 2);
        assert_eq!(list[1].taken_at, Some(500));
        assert_eq!(list[2].taken_at, Some(500));
    }

    #[test]
    fn library_raw_under_raw_subdir_pairs_with_its_bitmap() {
        // 库内布局：photos/<day>/MYP1.JPG + photos/<day>/_RAW/MYP1.RW2（memory/FUNCTION-REPOSITORY.md §4.1）
        let mut list = pairs(&[
            ("/lib/photos/2026-09-13/MYP1000035.JPG", false, Some(7000)),
            ("/lib/photos/2026-09-13/_RAW/MYP1000035.RW2", true, None),
        ]);
        assert_eq!(inherit_times_from_bitmaps(&mut list), 1);
        assert_eq!(list[1].taken_at, Some(7000));
    }

    #[test]
    fn different_directories_never_pair() {
        // 目录不同 → 不许互相借（同名不同目录是两张照片）
        let mut list = pairs(&[
            ("/src/a/P1.JPG", false, Some(100)),
            ("/src/b/P1.RW2", true, None),
        ]);
        assert_eq!(inherit_times_from_bitmaps(&mut list), 0);
        assert_eq!(list[1].taken_at, None);
    }

    #[test]
    fn missing_bitmap_or_bitmap_without_time_leaves_raw_alone() {
        let mut list = pairs(&[
            ("/src/P1.RW2", true, None),               // 没有位图
            ("/src/P2.JPG", false, None),              // 位图自己也没时间
            ("/src/P2.RW2", true, None),
        ]);
        assert_eq!(inherit_times_from_bitmaps(&mut list), 0);
        assert_eq!(list[0].taken_at, None);
        assert_eq!(list[2].taken_at, None);
    }

    #[test]
    fn bitmap_with_time_is_never_overwritten_and_earliest_wins() {
        // 多张同键位图（例如同名的 JPG + TIFF）：取最早的那个，且不改动它们自己
        let mut list = pairs(&[
            ("/src/P1.RW2", true, None),
            ("/src/P1.JPG", false, Some(900)),
            ("/src/P1.TIF", false, Some(300)),
        ]);
        assert_eq!(inherit_times_from_bitmaps(&mut list), 1);
        assert_eq!(list[0].taken_at, Some(300), "取最早的那张位图");
        assert_eq!(list[1].taken_at, Some(900), "位图自己的时间不许被改");
        assert_eq!(list[2].taken_at, Some(300));
    }

    #[test]
    fn raw_with_its_own_time_keeps_it() {
        let mut list = pairs(&[
            ("/src/P1.JPG", false, Some(900)),
            ("/src/P1.RW2", true, Some(899)),
        ]);
        assert_eq!(inherit_times_from_bitmaps(&mut list), 0);
        assert_eq!(list[1].taken_at, Some(899), "自己读到了就用自己读到的");
    }

    #[test]
    fn empty_input_and_rootless_paths_do_not_panic() {
        let mut empty: Vec<TimePair> = Vec::new();
        assert_eq!(inherit_times_from_bitmaps(&mut empty), 0);
        let mut bare = pairs(&[("P1.JPG", false, Some(1)), ("P1.RW2", true, None)]);
        assert_eq!(inherit_times_from_bitmaps(&mut bare), 1);
        assert_eq!(bare[1].taken_at, Some(1));
        assert_eq!(pair_key(Path::new("")), "");
    }
}
