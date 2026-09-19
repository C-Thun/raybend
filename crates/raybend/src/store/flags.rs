//! 旗标：**只活在内存里**的临时工作集。
//!
//! 出处：[`BROWSE.md`] §3.2 —— 用户 2026-09-15 明确「旗标不会被持久化，仅用于临时标记，
//! 在内存中保存，可以跨库跨目录，一定程度代替未来规划的 picture bucket」。
//! 所以这里是一个纯内存结构，**不碰数据库**（`AGENTS.md` §11.4、`FUTURE.md` H6）。
//!
//! # 为什么键要带库 ID
//!
//! 「跨库」意味着同一个 `FlagSet` 里会同时装着不同库的照片。而 `assets.id` 是
//! **每库自增**的 —— A 库的 5 号和 B 库的 5 号是两张不同的照片。直接用 `i64` 当键，
//! 两个库的照片会互相冒充。所以键是 `(repository_id, asset_id)`。
//!
//! # 旗标只有两态
//!
//! `Pick`（留）与 `Reject`（弃）。`BROWSE.md` §3.2 写的是「开 / 关（只有两态，明确）」，
//! 这里实现成**互斥**的两态：一张照片要么被留下、要么被弃掉、要么没被标 ——
//! 不会同时是两者（同时是两者在界面上没法表达）。

use std::collections::HashMap;

/// 一页照片在旗标上的身份：**库 + 库内 id**。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlagKey {
    pub repository_id: String,
    pub asset_id: i64,
}

impl FlagKey {
    #[must_use]
    pub fn new(repository_id: impl Into<String>, asset_id: i64) -> Self {
        Self {
            repository_id: repository_id.into(),
            asset_id,
        }
    }
}

/// 旗标的两态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// 留下（要通过的）。
    Pick,
    /// 弃掉（不要的）。
    Reject,
}

impl Flag {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pick => "pick",
            Self::Reject => "reject",
        }
    }
}

/// 内存里的旗标集合。
#[derive(Debug, Default, Clone)]
pub struct FlagSet {
    marks: HashMap<FlagKey, Flag>,
}

impl FlagSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 打旗标；`None` 表示清掉这一张的旗标。
    pub fn set(&mut self, key: FlagKey, flag: Option<Flag>) {
        match flag {
            Some(flag) => {
                self.marks.insert(key, flag);
            }
            None => {
                self.marks.remove(&key);
            }
        }
    }

    /// 批量打同一种旗标。
    pub fn set_many(&mut self, keys: impl IntoIterator<Item = FlagKey>, flag: Flag) {
        for key in keys {
            self.marks.insert(key, flag);
        }
    }

    /// 这一张的旗标。
    #[must_use]
    pub fn get(&self, key: &FlagKey) -> Option<Flag> {
        self.marks.get(key).copied()
    }

    /// 这一张是不是被留下了。
    #[must_use]
    pub fn is_picked(&self, key: &FlagKey) -> bool {
        self.get(key) == Some(Flag::Pick)
    }

    /// 这一张是不是被弃了。
    #[must_use]
    pub fn is_rejected(&self, key: &FlagKey) -> bool {
        self.get(key) == Some(Flag::Reject)
    }

    /// 清空**所有**旗标（用户点「清空旗标」，`BROWSE.md` §3.2 要求先确认）。
    ///
    /// 返回清掉了多少个 —— 调用方可以据此提示「已移除 12 个旗标」。
    pub fn clear(&mut self) -> usize {
        let n = self.marks.len();
        self.marks.clear();
        n
    }

    /// 只清某一个库的旗标（库被移除时用）。
    pub fn clear_repository(&mut self, repository_id: &str) -> usize {
        let before = self.marks.len();
        self.marks
            .retain(|key, _| key.repository_id != repository_id);
        before - self.marks.len()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.marks.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    /// 某种旗标有几个。
    #[must_use]
    pub fn count(&self, flag: Flag) -> usize {
        self.marks.values().filter(|f| **f == flag).count()
    }

    /// 全部旗标（调试 / 导出用）。
    #[must_use]
    pub fn all(&self) -> &HashMap<FlagKey, Flag> {
        &self.marks
    }

    /// 按旗标筛出一批 id（**只认这一个库的**）。
    ///
    /// 用途：`BROWSE.md` §3.1 的筛选 —— 旗标不进 SQL（库里没有这一列），
    /// 所以「只看旗标」这一步在拿到查询结果之后由前端（或这里）过滤。
    #[must_use]
    pub fn filter_ids(&self, repository_id: &str, flag: Flag) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .marks
            .iter()
            .filter(|(key, f)| key.repository_id == repository_id && **f == flag)
            .map(|(key, _)| key.asset_id)
            .collect();
        ids.sort_unstable();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(asset_id: i64) -> FlagKey {
        FlagKey::new("RepoA", asset_id)
    }

    #[test]
    fn starts_empty() {
        let set = FlagSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert_eq!(set.get(&key(1)), None);
    }

    #[test]
    fn setting_and_clearing_one_photo() {
        let mut set = FlagSet::new();
        set.set(key(1), Some(Flag::Pick));
        assert!(set.is_picked(&key(1)));
        assert!(!set.is_rejected(&key(1)));

        set.set(key(1), Some(Flag::Reject));
        assert!(set.is_rejected(&key(1)), "两态互斥：改了就是改了");
        assert!(!set.is_picked(&key(1)));

        set.set(key(1), None);
        assert_eq!(set.get(&key(1)), None);
        assert!(set.is_empty());
    }

    #[test]
    fn the_same_asset_id_in_two_repositories_are_different_photos() {
        let mut set = FlagSet::new();
        set.set(FlagKey::new("RepoA", 5), Some(Flag::Pick));
        set.set(FlagKey::new("RepoB", 5), Some(Flag::Reject));

        assert!(set.is_picked(&FlagKey::new("RepoA", 5)));
        assert!(set.is_rejected(&FlagKey::new("RepoB", 5)));
        assert_eq!(set.len(), 2, "两个库的同号照片各算各的");
        assert_eq!(set.count(Flag::Pick), 1);
        assert_eq!(set.count(Flag::Reject), 1);
    }

    #[test]
    fn batch_setting_applies_to_all() {
        let mut set = FlagSet::new();
        set.set_many([key(1), key(2), key(3)], Flag::Reject);
        assert_eq!(set.count(Flag::Reject), 3);
        assert_eq!(set.filter_ids("RepoA", Flag::Reject), vec![1, 2, 3]);
    }

    #[test]
    fn clearing_everything_reports_how_many_were_removed() {
        let mut set = FlagSet::new();
        set.set_many([key(1), FlagKey::new("RepoB", 9)], Flag::Pick);
        assert_eq!(set.clear(), 2);
        assert!(set.is_empty());
        assert_eq!(set.clear(), 0, "再清一次是 0，不是错误");
    }

    #[test]
    fn clearing_one_repository_keeps_the_others() {
        let mut set = FlagSet::new();
        set.set(FlagKey::new("RepoA", 1), Some(Flag::Pick));
        set.set(FlagKey::new("RepoA", 2), Some(Flag::Pick));
        set.set(FlagKey::new("RepoB", 1), Some(Flag::Pick));

        assert_eq!(set.clear_repository("RepoA"), 2);
        assert_eq!(set.len(), 1);
        assert!(set.is_picked(&FlagKey::new("RepoB", 1)));
    }

    #[test]
    fn filter_ids_is_scoped_and_sorted() {
        let mut set = FlagSet::new();
        set.set(FlagKey::new("RepoA", 9), Some(Flag::Pick));
        set.set(FlagKey::new("RepoA", 3), Some(Flag::Pick));
        set.set(FlagKey::new("RepoA", 5), Some(Flag::Reject));
        set.set(FlagKey::new("RepoB", 1), Some(Flag::Pick));

        assert_eq!(set.filter_ids("RepoA", Flag::Pick), vec![3, 9]);
        assert_eq!(set.filter_ids("RepoA", Flag::Reject), vec![5]);
        assert!(set.filter_ids("RepoZ", Flag::Pick).is_empty());
    }

    #[test]
    fn flag_names_are_stable() {
        // 会进日志与 IPC
        assert_eq!(Flag::Pick.as_str(), "pick");
        assert_eq!(Flag::Reject.as_str(), "reject");
    }
}
