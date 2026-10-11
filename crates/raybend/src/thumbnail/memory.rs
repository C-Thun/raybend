//! **库外（未入库）图的进程内缓存**（崔总 2026-10-11 定的口径；`AGENTS.md` §2.24）。
//!
//! # 为什么不用磁盘缓存
//!
//! import 面对的是**临时目录**（相机卡 / 待入老库 / 临时盘）：同一个路径下次打开，
//! 里面的照片可能完全不同。落盘的行**永远读不回来**（内容一变签名就变），
//! 剩下的只有无限增长的垃圾。所以库外的图**只在进程内存里** ——
//! 进程退出即清，这正是「临时」该有的样子。
//!
//! 库内（入库资产）不走这里：它们照旧落 `_sources/thumbs.db`，
//! 重启后浏览还得靠那份命中（`AGENTS.md` §6.5）。
//!
//! # 键：路径 + 尺度 + 渲染签名 + **文件身份**
//!
//! 文件身份 = **大小 + 修改时间**（与 `crate::media::meta_cache` 判定
//! 「是不是同一个文件」的口径一致）。换卡、覆盖保存、同名不同内容 → 键不同 →
//! 自动不命中，**不需要任何失效钩子**：拿不到旧图，也就没有「图错了」。
//!
//! # 两档（崔总 2026-10-11 定；`specs/thumbnail-tiers-and-import-memory.md`）
//!
//! | 档 | 封顶方式 | 默认 | 谁在用 |
//! | --- | --- | --- | --- |
//! | 缩略图（384，网格与胶片带共用） | 按**字节** | [`DEFAULT_SMALL_BYTES`] = 256 MB（≈ 9500 张） | tiles、胶片带 |
//! | 真身（该图自己的像素） | 按**条数** | [`DEFAULT_FULL_ENTRIES`] = 10 张（≈ 90 MB） | import 的看图终态 |
//!
//! 两档**各有各的池子** —— 真身一张顶小图三百多张，混在一起会互相挤。
//! 上限不是预留：LRU 按需长，看 300 张的目录只会占十来兆。
//!
//! **没有 1920（`Screen`）这一档**：import 不生成、不请求它（占位由 384 顶着），
//! 所以库外那条路走到 `Screen` 就直接渲染、不缓存（`Class::of` 返回 `None`）。
//!
//! 这是**渲染缓存**，不是显示缓存：前端那份 blob 缓存（`thumb-queue.ts`）管的是
//! 「同一屏来回滚不用重新要字节」，两份各司其职，都不是「同一能力的第二份实现」。

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use crate::error::Result;
use crate::thumbnail::render::{SizeClass, Thumb};

/// 缩略图档（384）的**字节**预算（≈ 9500 张 384px JPEG q82）。
pub const DEFAULT_SMALL_BYTES: usize = 256 * 1024 * 1024;

/// **真身**档的**条数**预算（一张 ~9MB，10 张 ≈ 90 MB）。
pub const DEFAULT_FULL_ENTRIES: usize = 10;

/// 两档的预算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub small_bytes: usize,
    pub full_entries: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            small_bytes: DEFAULT_SMALL_BYTES,
            full_entries: DEFAULT_FULL_ENTRIES,
        }
    }
}

/// 一条缓存的键。
///
/// `len` / `modified_ms` 是**文件身份**：`None` 表示读不到元数据（文件没了 / 没权限），
/// 那种情况不缓存 —— 反正渲染也拿不到东西。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceKey {
    path: PathBuf,
    size: SizeClass,
    sig: &'static str,
    stamp: Option<Stamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Stamp {
    len: u64,
    /// 修改时间的**毫秒**（亚毫秒抖动对「是不是同一个文件」没有意义）。
    modified_ms: i64,
}

impl SourceKey {
    /// 按现在这一刻的文件事实做一把键。
    #[must_use]
    pub fn of(path: &Path, size: SizeClass, sig: &'static str) -> Self {
        Self {
            path: path.to_path_buf(),
            size,
            sig,
            stamp: stamp_of(path),
        }
    }

    /// 文件身份读到了吗（读不到就别进缓存）。
    #[must_use]
    pub const fn is_stamped(&self) -> bool {
        self.stamp.is_some()
    }

    /// 这条键的尺度。
    #[must_use]
    pub const fn size(&self) -> SizeClass {
        self.size
    }
}

/// 文件身份：**大小 + 修改时间**（一次 `stat`，不打开文件）。
fn stamp_of(path: &Path) -> Option<Stamp> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let modified_ms = match modified.duration_since(UNIX_EPOCH) {
        Ok(delta) => i64::try_from(delta.as_millis()).unwrap_or(i64::MAX),
        // 早于 1970 的文件系统时间：取负，保证不同的时间仍是不同的键
        Err(error) => -i64::try_from(error.duration().as_millis()).unwrap_or(i64::MAX),
    };
    Some(Stamp {
        len: metadata.len(),
        modified_ms,
    })
}

/// 进哪一档预算；`None` = 这一档不进内存缓存。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Small,
    Full,
}

impl Class {
    fn of(size: SizeClass) -> Option<Self> {
        match size {
            SizeClass::Grid => Some(Self::Small),
            SizeClass::Full => Some(Self::Full),
            // 1920：import 不生成也不请求这一档（占位用 384），所以不缓存 ——
            // 真有人请求就现算，别在内存里留一份没人再要的 1920。
            SizeClass::Screen => None,
        }
    }
}

#[derive(Debug)]
struct Entry {
    class: Class,
    thumb: Thumb,
    used: u64,
}

#[derive(Debug)]
struct Inner {
    entries: HashMap<SourceKey, Entry>,
    /// LRU 顺序按档分开：`used` 单调递增，最前面的是最久没用的
    small_order: BTreeMap<u64, SourceKey>,
    full_order: BTreeMap<u64, SourceKey>,
    tick: u64,
    budget: Budget,
    /// 缩略图档当前占用的字节
    used_bytes: usize,
    /// 真身档当前存了几条
    used_fulls: usize,
    hits: u64,
    misses: u64,
}

/// 缓存现状（排错与探针用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryStats {
    /// 两档一共几条
    pub entries: usize,
    /// 两档一共占多少字节
    pub bytes: usize,
    /// 缩略图档当前占的字节 / 预算字节
    pub small_bytes: usize,
    pub small_budget_bytes: usize,
    /// 真身档当前条数 / 预算条数
    pub full_entries: usize,
    pub full_budget: usize,
    pub hits: u64,
    pub misses: u64,
}

/// 库外图的进程内缓存（线程安全；跟着 app 活，退出即清）。
#[derive(Debug)]
pub struct SourceThumbs {
    inner: Mutex<Inner>,
    budget: Budget,
}

impl Default for SourceThumbs {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceThumbs {
    #[must_use]
    pub fn new() -> Self {
        Self::with_budget(Budget::default())
    }

    #[must_use]
    pub fn with_budget(budget: Budget) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                small_order: BTreeMap::new(),
                full_order: BTreeMap::new(),
                tick: 0,
                budget,
                used_bytes: 0,
                used_fulls: 0,
                hits: 0,
                misses: 0,
            }),
            budget,
        }
    }

    #[must_use]
    pub const fn budget(&self) -> Budget {
        self.budget
    }

    /// 取一条（命中会把它挪到 LRU 最新端）。
    pub fn get(&self, key: &SourceKey) -> Option<Thumb> {
        let mut inner = self.lock();
        let thumb = inner.entries.get(key).map(|entry| entry.thumb.clone())?;
        inner.touch(key);
        inner.hits += 1;
        Some(thumb)
    }

    /// 放一条（超预算就按 LRU 淘汰）。
    pub fn put(&self, key: SourceKey, thumb: Thumb) {
        let Some(class) = Class::of(key.size) else {
            return;
        };
        if !key.is_stamped() {
            return;
        }
        let mut inner = self.lock();
        inner.insert(key, class, thumb);
        inner.evict(class);
    }

    /// 命中就用内存里的；没命中才渲染，并把结果留下。
    ///
    /// 渲染在锁**外面**跑（一张 384px 要 200ms，攥着锁会把别的请求堵死）；
    /// 并发请求同一个文件时可能各渲染一次 —— 前端的队列已经按路径去重，
    /// 这里不再引一层「在飞清单」。
    ///
    /// # Errors
    /// 渲染失败（文件读不了 / 解不开）。
    pub fn get_or_render(
        &self,
        key: &SourceKey,
        render: impl FnOnce() -> Result<Thumb>,
    ) -> Result<Thumb> {
        {
            let mut inner = self.lock();
            if let Some(entry) = inner.entries.get(key) {
                let thumb = entry.thumb.clone();
                inner.touch(key);
                inner.hits += 1;
                return Ok(thumb);
            }
            inner.misses += 1;
        }
        let thumb = render()?;
        self.put(key.clone(), thumb.clone());
        Ok(thumb)
    }

    #[must_use]
    pub fn stats(&self) -> MemoryStats {
        let inner = self.lock();
        MemoryStats {
            entries: inner.entries.len(),
            bytes: inner.entries.values().map(|e| e.thumb.data.len()).sum(),
            small_bytes: inner.used_bytes,
            small_budget_bytes: inner.budget.small_bytes,
            full_entries: inner.used_fulls,
            full_budget: inner.budget.full_entries,
            hits: inner.hits,
            misses: inner.misses,
        }
    }

    /// 清空（测试与将来的「清缓存」用；正常路径不需要 —— 键自带身份，不会拿错图）。
    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.entries.clear();
        inner.small_order.clear();
        inner.full_order.clear();
        inner.used_bytes = 0;
        inner.used_fulls = 0;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // 锁中毒（某个线程 panic）不该让取图整条路瘫掉：拿回内部数据继续用。
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Inner {
    fn touch(&mut self, key: &SourceKey) {
        let Some(class) = self.entries.get(key).map(|entry| entry.class) else {
            return;
        };
        self.tick += 1;
        let tick = self.tick;
        if let Some(entry) = self.entries.get_mut(key) {
            let previous = entry.used;
            entry.used = tick;
            self.order_mut(class).remove(&previous);
        }
        self.order_mut(class).insert(tick, key.clone());
    }

    fn insert(&mut self, key: SourceKey, class: Class, thumb: Thumb) {
        self.tick += 1;
        let tick = self.tick;
        if let Some(previous) = self.entries.remove(&key) {
            self.order_mut(previous.class).remove(&previous.used);
            if previous.class == Class::Small {
                self.used_bytes = self.used_bytes.saturating_sub(previous.thumb.data.len());
            } else {
                self.used_fulls = self.used_fulls.saturating_sub(1);
            }
        }
        if class == Class::Small {
            self.used_bytes += thumb.data.len();
        } else {
            self.used_fulls += 1;
        }
        self.order_mut(class).insert(tick, key.clone());
        self.entries.insert(
            key,
            Entry {
                class,
                thumb,
                used: tick,
            },
        );
    }

    fn evict(&mut self, class: Class) {
        loop {
            let over = match class {
                Class::Small => self.used_bytes > self.budget.small_bytes,
                Class::Full => self.used_fulls > self.budget.full_entries,
            };
            if !over {
                return;
            }
            let Some(oldest) = self.order_mut(class).keys().next().copied() else {
                return;
            };
            let Some(key) = self.order_mut(class).remove(&oldest) else {
                return;
            };
            if let Some(entry) = self.entries.remove(&key) {
                if class == Class::Small {
                    self.used_bytes = self.used_bytes.saturating_sub(entry.thumb.data.len());
                } else {
                    self.used_fulls = self.used_fulls.saturating_sub(1);
                }
            }
        }
    }

    fn order_mut(&mut self, class: Class) -> &mut BTreeMap<u64, SourceKey> {
        match class {
            Class::Small => &mut self.small_order,
            Class::Full => &mut self.full_order,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thumb(bytes: usize) -> Thumb {
        Thumb {
            data: vec![7u8; bytes],
            width: 384,
            height: 256,
            placeholder: false,
        }
    }

    fn key(path: &str, size: SizeClass) -> SourceKey {
        SourceKey::of(Path::new(path), size, "jpeg-q82-grid-v16")
    }

    /// 建一个带真文件的小目录，让 `stamp` 有东西可读。
    fn photo(dir: &Path, name: &str, bytes: usize) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, vec![1u8; bytes]).unwrap();
        path
    }

    #[test]
    fn put_then_get_returns_the_same_thumb() {
        let dir = tempfile::tempdir().unwrap();
        let path = photo(dir.path(), "a.jpg", 16);
        let cache = SourceThumbs::new();
        let k = SourceKey::of(&path, SizeClass::Grid, "sig");

        assert!(cache.get(&k).is_none());
        cache.put(k.clone(), thumb(32));
        let hit = cache.get(&k).unwrap();
        assert_eq!(hit.data.len(), 32);
        assert_eq!(hit.width, 384);
        let stats = cache.stats();
        assert_eq!((stats.entries, stats.hits, stats.misses), (1, 1, 0));
    }

    #[test]
    fn a_changed_file_is_a_different_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = photo(dir.path(), "a.jpg", 16);
        let cache = SourceThumbs::new();
        let before = SourceKey::of(&path, SizeClass::Grid, "sig");
        cache.put(before.clone(), thumb(32));

        // 同一路径、内容换了（大小也换）——**不许**拿旧图
        photo(dir.path(), "a.jpg", 4096);
        let after = SourceKey::of(&path, SizeClass::Grid, "sig");
        assert_ne!(before, after, "文件身份变了，键就该变");
        assert!(cache.get(&after).is_none(), "换了内容不能命中旧图");

        // 新内容自己再存一次，命中自己那份
        cache.put(after.clone(), thumb(64));
        assert!(cache.get(&after).is_some());
        // 旧身份那条还在缓存里（LRU 会把它淘汰掉），但**永远不会被请求到**
        assert_ne!(before, after);
    }

    #[test]
    fn a_missing_file_is_never_cached() {
        let cache = SourceThumbs::new();
        let k = SourceKey::of(Path::new("/no/such/photo.jpg"), SizeClass::Grid, "sig");
        assert!(!k.is_stamped());
        cache.put(k.clone(), thumb(32));
        assert_eq!(cache.stats().entries, 0, "身份读不到的条目不进缓存");
        assert!(cache.get(&k).is_none());
    }

    #[test]
    fn small_tier_evicts_by_bytes_least_recently_used_first() {
        let dir = tempfile::tempdir().unwrap();
        let a = photo(dir.path(), "a.jpg", 8);
        let b = photo(dir.path(), "b.jpg", 8);
        let c = photo(dir.path(), "c.jpg", 8);
        let cache = SourceThumbs::with_budget(Budget {
            small_bytes: 100,
            full_entries: 4,
        });
        let (ka, kb, kc) = (
            SourceKey::of(&a, SizeClass::Grid, "sig"),
            SourceKey::of(&b, SizeClass::Grid, "sig"),
            SourceKey::of(&c, SizeClass::Grid, "sig"),
        );
        cache.put(ka.clone(), thumb(40));
        cache.put(kb.clone(), thumb(40));
        // 用一下 a，让 b 变成最久没用过的
        assert!(cache.get(&ka).is_some());
        cache.put(kc.clone(), thumb(40));

        assert_eq!(cache.stats().small_bytes, 80, "超预算就把最久没用的挤出去");
        assert!(cache.get(&kb).is_none(), "b 是最久没用的");
        assert!(cache.get(&ka).is_some());
        assert!(cache.get(&kc).is_some());
    }

    #[test]
    fn full_tier_has_its_own_budget_and_never_squeezes_small_ones() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SourceThumbs::with_budget(Budget {
            small_bytes: 1024,
            full_entries: 2,
        });
        let grid = dir.path().join("grid.jpg");
        std::fs::write(&grid, b"x").unwrap();
        cache.put(SourceKey::of(&grid, SizeClass::Grid, "sig"), thumb(100));

        let mut fulls = Vec::new();
        for index in 0..3 {
            let path = dir.path().join(format!("s{index}.jpg"));
            std::fs::write(&path, b"x").unwrap();
            let key = SourceKey::of(&path, SizeClass::Full, "sig");
            cache.put(key.clone(), thumb(500));
            fulls.push(key);
        }
        let stats = cache.stats();
        assert_eq!(stats.full_entries, 2, "真身档按条数封顶（崔总 2026-10-11：10 张）");
        assert_eq!(stats.small_bytes, 100, "缩略图档按字节算，不被真身挤");
        assert_eq!(stats.bytes, 100 + 1000, "两档各占各的");
        assert!(cache.get(&fulls[0]).is_none(), "最久没用的先走");
        assert!(cache.get(&fulls[2]).is_some());
        assert!(cache.get(&SourceKey::of(&grid, SizeClass::Grid, "sig")).is_some());
    }

    #[test]
    fn the_1920_tier_is_never_cached_here() {
        // import 不生成也不请求 1920（占位用 384）：内存缓存就没有这一档
        let dir = tempfile::tempdir().unwrap();
        let path = photo(dir.path(), "a.jpg", 8);
        let cache = SourceThumbs::new();
        let key = SourceKey::of(&path, SizeClass::Screen, "sig");
        cache.put(key.clone(), thumb(1024));
        assert_eq!(cache.stats().entries, 0);
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn get_or_render_renders_once_then_serves_from_memory() {
        let dir = tempfile::tempdir().unwrap();
        let path = photo(dir.path(), "a.jpg", 8);
        let cache = SourceThumbs::new();
        let key = SourceKey::of(&path, SizeClass::Grid, "sig");
        let mut renders = 0;
        let first = cache
            .get_or_render(&key, || {
                renders += 1;
                Ok(thumb(24))
            })
            .unwrap();
        let second = cache
            .get_or_render(&key, || {
                renders += 1;
                Err(crate::error::Error::Unsupported("不该再渲染".into()))
            })
            .unwrap();
        assert_eq!(renders, 1, "第二次必须命中内存");
        assert_eq!(first, second);
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn clear_drops_everything() {
        let dir = tempfile::tempdir().unwrap();
        let path = photo(dir.path(), "a.jpg", 8);
        let cache = SourceThumbs::new();
        cache.put(SourceKey::of(&path, SizeClass::Grid, "sig"), thumb(16));
        cache.clear();
        let stats = cache.stats();
        assert_eq!((stats.entries, stats.bytes, stats.full_entries), (0, 0, 0));
        assert_eq!(stats.small_bytes, 0);
    }
}
