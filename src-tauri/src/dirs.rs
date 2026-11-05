//! 库内目录的两个命令：**查看能不能删** 与 **删除**（崔总 2026-11-05 把行尾 `⋯` 菜单
//! 拆成左列常驻工具栏之后，入口换成了菜单区那颗按钮）。
//!
//! 规则与底线全在 `raybend::repo::dirs` 里（那里有单测），这里只做三件事：
//! 把库内相对路径解析成绝对路径并挡住越界、把结果转成前端视图、把磁盘操作放到后台线程。
//!
//! ⚠️ 前端传的是 `root + rel` 而不是绝对路径：`rel` 会经过 `resolve_inside`，
//! `..` 与绝对路径一律拒绝 —— 免得界面上一处笔误就动到库外面去。
//!
//! 判据是**浅检查**（只看这一层的目录与照片），所以前端可以在选中目录时随时跑一次
//! 给按钮定 enable / disable，不必等用户点开菜单（见 `repo::dirs` 文件头）。

use std::path::Path;

use raybend::repo::dirs;
use serde::Serialize;

use crate::source::blocking;

/// 「这个目录能不能删」的视图（camelCase，进 `src/api/dto-contract.json`）。
///
/// `Default` 是契约测试（`contract.rs` 的 `keys_of`）要求的：那里用一个默认实例
/// 序列化出来对键名，少写一个键就是「界面上永远空着」那类静默故障。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirDeleteCheckView {
    /// 能不能删（= 看不见照片、看不见子目录、没有链接）
    pub deletable: bool,
    /// 本层 + `_RAW/` 里的照片数（用来解释「为什么不能删」）
    pub photo_count: u64,
    /// 可见子目录数（不递归）
    pub visible_dir_count: u64,
    /// 有符号链接（保守：算有东西）
    pub has_link: bool,
}

/// 浅检查：这个目录现在能不能删（决定工具栏「删除目录」那颗按钮的 enable）。
#[tauri::command]
pub async fn dir_delete_check(root: String, rel: String) -> Result<DirDeleteCheckView, String> {
    blocking(move || {
        let target = dirs::resolve_inside(Path::new(&root), &rel)?;
        let report =
            dirs::check_visible_contents(&target).map_err(|e| format!("检查目录失败：{e}"))?;
        Ok(DirDeleteCheckView {
            deletable: report.deletable(),
            photo_count: report.photo_count,
            visible_dir_count: report.visible_dir_count,
            has_link: report.has_link,
        })
    })
    .await
}

/// 删除目录（连同看不见的残留）。
///
/// **删前自己再查一遍**（`remove_dir_checked`）：界面上的 enable 与这里用的是同一个函数，
/// 但两次调用之间有真实的时间差（可能刚好有人往里丢了张照片）。
#[tauri::command]
pub async fn dir_delete(root: String, rel: String) -> Result<u64, String> {
    blocking(move || {
        let target = dirs::resolve_inside(Path::new(&root), &rel)?;
        dirs::remove_dir_checked(&target).map_err(|e| format!("删除目录失败：{e}"))
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
            tauri::async_runtime::block_on(dir_delete_check(root_s.clone(), "../..".into()))
                .is_err()
        );
        assert!(
            tauri::async_runtime::block_on(dir_delete(root_s.clone(), "..".into())).is_err()
        );
        assert!(
            tauri::async_runtime::block_on(dir_create(root_s, "..".into(), "x".into())).is_err()
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn delete_check_agrees_with_delete() {
        let root = temp_root("delete-check");
        let root_s = root.to_string_lossy().into_owned();
        std::fs::create_dir_all(root.join("photos/空目录")).unwrap();
        let view = tauri::async_runtime::block_on(dir_delete_check(
            root_s.clone(),
            "photos/空目录".into(),
        ))
        .unwrap();
        assert!(view.deletable);
        assert_eq!(view.photo_count, 0);
        assert_eq!(
            tauri::async_runtime::block_on(dir_delete(root_s, "photos/空目录".into())).unwrap(),
            1
        );
        assert!(!root.join("photos/空目录").exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
