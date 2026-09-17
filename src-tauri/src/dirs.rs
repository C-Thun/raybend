//! 库内目录树的菜单命令（`BROWSE.md` §4.3 行尾 `⋯`）：**删除空目录**与**创建子目录**。
//!
//! 规则与底线全在 `raybend::repo::dirs` 里（那里有单测），这里只做三件事：
//! 把库内相对路径解析成绝对路径并挡住越界、把结果转成前端视图、把磁盘操作放到后台线程。
//!
//! ⚠️ 前端传的是 `root + rel` 而不是绝对路径：`rel` 会经过 `resolve_inside`，
//! `..` 与绝对路径一律拒绝 —— 免得界面上一处笔误就动到库外面去。

use std::path::Path;

use raybend::repo::dirs;
use serde::Serialize;

use crate::source::blocking;

/// 深度空检查的结果（camelCase，进 `src/api/dto-contract.json`）。
///
/// `Default` 是契约测试（`contract.rs` 的 `keys_of`）要求的：那里用一个默认实例
/// 序列化出来对键名，少写一个键就是「界面上永远空着」那类静默故障。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEmptyView {
    /// 整个子树里一个文件都没有
    pub empty: bool,
    /// 子树里的文件数（用来向用户解释「为什么不能删」）
    pub file_count: u64,
    pub dir_count: u64,
    pub empty_dir_count: u64,
    /// 遇到符号链接之类不好判断的东西（保守判成不空）
    pub has_unresolved_link: bool,
}

/// 深度检查：这个目录的整棵子树里有没有文件（决定「删除空目录」能不能点）。
#[tauri::command]
pub async fn dir_empty_check(root: String, rel: String) -> Result<DirEmptyView, String> {
    blocking(move || {
        let target = dirs::resolve_inside(Path::new(&root), &rel)?;
        let report = dirs::check_empty_tree(&target).map_err(|e| format!("检查目录失败：{e}"))?;
        Ok(DirEmptyView {
            empty: report.empty,
            file_count: report.file_count,
            dir_count: report.dir_count,
            empty_dir_count: report.empty_dir_count,
            has_unresolved_link: report.has_unresolved_link,
        })
    })
    .await
}

/// 删除空目录（连同其下所有空子目录），返回删掉的目录个数。
///
/// **只删空目录**：底层只调 `remove_dir`，目录非空时必然失败并报错 ——
/// 这条路径不可能删到用户的照片。
#[tauri::command]
pub async fn dir_remove_empty(root: String, rel: String) -> Result<u64, String> {
    blocking(move || {
        let target = dirs::resolve_inside(Path::new(&root), &rel)?;
        let removed = dirs::remove_empty_tree(&target).map_err(|e| format!("删除目录失败：{e}"))?;
        Ok(removed as u64)
    })
    .await
}

/// 在某个目录下建一个子目录，返回它的**库内相对路径**（前端拿去在树里定位）。
#[tauri::command]
pub async fn dir_create(root: String, rel: String, name: String) -> Result<String, String> {
    blocking(move || {
        let parent = dirs::resolve_inside(Path::new(&root), &rel)?;
        dirs::create_subdir(&parent, &name)?;
        let rel = rel.trim_matches('/').replace('\\', "/");
        Ok(if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("raybend-dirscmd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_returns_relative_path() {
        let root = temp_root("rel");
        let root_s = root.to_string_lossy().into_owned();
        // 库根下
        assert_eq!(
            tauri::async_runtime::block_on(dir_create(root_s.clone(), String::new(), "a".into()))
                .unwrap(),
            "a"
        );
        // 子目录下（顺带验证 rel 里的反斜杠会被归一成 /）—— 父目录要先存在：
        // `create_subdir` 刻意**不替用户造一串**（父目录不在就报错）
        std::fs::create_dir_all(root.join("photos")).unwrap();
        assert_eq!(
            tauri::async_runtime::block_on(dir_create(root_s, "photos".into(), "b".into()))
                .unwrap(),
            "photos/b"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn commands_reject_escape() {
        let root = temp_root("escape");
        let root_s = root.to_string_lossy().into_owned();
        assert!(
            tauri::async_runtime::block_on(dir_empty_check(root_s.clone(), "../..".into())).is_err()
        );
        assert!(
            tauri::async_runtime::block_on(dir_remove_empty(root_s.clone(), "..".into())).is_err()
        );
        assert!(
            tauri::async_runtime::block_on(dir_create(root_s, "..".into(), "x".into())).is_err()
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
