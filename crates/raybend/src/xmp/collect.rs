//! 从 `catalog.db` 组装一个资产的 sidecar **落点与内容**（`specs/xmp-sidecar.md` §2/§4/§5）。
//!
//! **唯一一份**：运行时写出口（`src-tauri/src/sidecar.rs`）与巡检工具
//! （`xmp::audit`，`specs/xmp-audit-tool.md`）都用这里的组装结果 ——
//! 两套口径迟早会漂移，而漂移的后果是「盘上文件与库不一致却没人发现」（`AGENTS.md` §2.12）。

use rusqlite::Connection;

use super::{SidecarContent, SidecarMetadata, SidecarProfile};
use crate::Result;
use crate::store::{assets, develop, issues, photo_tags};

/// 一个资产的 sidecar 落点与内容。
#[derive(Debug, Clone)]
pub struct AssetSidecar {
    /// sidecar 的**库内相对路径**（`/` 分隔；`_RAW/` 已折算，规则见 [`super::path`]）。
    pub rel: String,
    /// 应当写出的内容（与数据库当前状态一致）。
    pub content: SidecarContent,
}

/// 读一个资产。`Ok(None)` = 连文件行都没有（资产已删/还没落库）——没有可镜像的东西。
pub fn collect(conn: &Connection, asset_id: i64) -> Result<Option<AssetSidecar>> {
    let files = assets::files_of_asset(conn, asset_id)?;
    let Some(rel) = super::sidecar_rel_path(&files) else {
        return Ok(None);
    };

    let latest = develop::load(conn, asset_id)?;
    let finalized = issues::list(conn, asset_id)?;
    let (rating, color_label, author, description, country, province_state, city, sublocation) =
        conn.query_row(
            "SELECT rating, color_label, author, description, country, province_state, city, sublocation \
             FROM assets WHERE id = ?1",
            [asset_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )?;
    let keywords = photo_tags::effective_names(conn, asset_id)?;
    let tag_state = photo_tags::snapshot(conn, asset_id)?;

    // `crs:RawFileName` 只写文件名：优先在线的那份 RAW，缺失的也认（路径还在库里）。
    let raw_file_name = files
        .iter()
        .find(|file| file.role == "raw" && !file.missing)
        .or_else(|| files.iter().find(|file| file.role == "raw"))
        .and_then(|file| file.rel_path.rsplit('/').next().map(str::to_string));

    Ok(Some(AssetSidecar {
        rel,
        content: SidecarContent {
            tag_state: Some(tag_state),
            latest: Some(latest),
            profiles: finalized
                .iter()
                .map(|issue| SidecarProfile {
                    name: issue.name.clone(),
                    source_base: issue.source_base,
                    created_at_ms: issue.created_at,
                    ordinal: Some(issue.ordinal),
                    stack: issue.stack.clone(),
                })
                .collect(),
            metadata: SidecarMetadata {
                rating: rating.clamp(0, 5) as u8,
                color_label,
                keywords,
                author,
                description,
                country,
                province_state,
                city,
                sublocation,
            },
            raw_file_name,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::develop::{DevelopStack, EditBase};
    use crate::store::migration::{self, DbKind};
    use crate::xmp::{compose, parse};

    /// 真实（内存）catalog 到最新 schema；与 `xmp::adopt` 的夹具同一套做法。
    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::store::pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), 1).unwrap();
        conn
    }

    /// 真实临时库 + 位图/RAW 文件行；返回（连接，资产 id，位图库内相对路径）。
    fn fixture(bitmap_rel: &str, raw_rel: Option<&str>) -> (Connection, i64, String) {
        let conn = db();
        let asset_id = assets::insert_asset(&conn, 10).unwrap();
        conn.execute(
            "INSERT INTO asset_files(asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at) \
             VALUES(?1, 'bitmap', ?2, lower(?2), 'jpg', 10, 10)",
            rusqlite::params![asset_id, bitmap_rel],
        )
        .unwrap();
        if let Some(raw) = raw_rel {
            conn.execute(
                "INSERT INTO asset_files(asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at) \
                 VALUES(?1, 'raw', ?2, lower(?2), 'orf', 10, 10)",
                rusqlite::params![asset_id, raw],
            )
            .unwrap();
        }
        (conn, asset_id, bitmap_rel.to_string())
    }

    #[test]
    fn collect_uses_bitmap_directory_and_raw_file_name() {
        let (conn, asset_id, _) = fixture(
            "photos/2026-08-15/MYP0001.JPG",
            Some("photos/2026-08-15/_RAW/MYP0001.ORF"),
        );
        conn.execute(
            "UPDATE assets SET rating = 4, color_label = 'green', city = '杭州' WHERE id = ?1",
            [asset_id],
        )
        .unwrap();

        let mut latest = DevelopStack {
            source_base: EditBase::Raw,
            ..Default::default()
        };
        latest.params.insert("exposure".into(), 0.35);
        develop::save(&conn, asset_id, &latest, 20).unwrap();
        issues::create(&conn, asset_id, "暖调", &latest, 30).unwrap();

        let asset = collect(&conn, asset_id).unwrap().expect("有文件行");
        assert_eq!(
            asset.rel, "photos/2026-08-15/MYP0001.xmp",
            "位图目录 + 折算 _RAW"
        );
        assert_eq!(
            asset.content.raw_file_name.as_deref(),
            Some("MYP0001.ORF"),
            "crs:RawFileName 只写文件名"
        );
        assert_eq!(
            asset
                .content
                .latest
                .as_ref()
                .unwrap()
                .params
                .get("exposure"),
            Some(&0.35)
        );
        assert_eq!(asset.content.profiles.len(), 1);
        assert_eq!(asset.content.metadata.rating, 4);
        assert_eq!(asset.content.metadata.city.as_deref(), Some("杭州"));

        // 组装出来的内容确实能写出一份可解析的文件（写出路径的下游契约）。
        let xml = compose(&asset.content, 40, "RayBend test").unwrap();
        let parsed = parse(&xml).unwrap().unwrap();
        assert_eq!(parsed.profiles.len(), 1);
        assert_eq!(parsed.metadata.rating, 4);
    }

    #[test]
    fn collect_is_none_without_file_rows() {
        let conn = db();
        let asset_id = assets::insert_asset(&conn, 10).unwrap();
        assert!(collect(&conn, asset_id).unwrap().is_none());
    }

    #[test]
    fn collected_content_publishes_and_clears_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("photos/2026-08-15")).unwrap();
        let (conn, asset_id, _) = fixture("photos/2026-08-15/MYP0001.JPG", None);

        // 有编辑 ⇒ 有内容（空 SOOC 栈也是内容，这里用真参数更接近实际）。
        let mut latest = DevelopStack {
            source_base: EditBase::Sooc,
            ..Default::default()
        };
        latest.params.insert("exposure".into(), 0.5);
        develop::save(&conn, asset_id, &latest, 20).unwrap();

        let asset = collect(&conn, asset_id).unwrap().expect("有内容");
        let abs = root.join(asset.rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let xml = compose(&asset.content, 40, "RayBend test").unwrap();
        assert_eq!(
            crate::xmp::publish(&abs, &xml).unwrap(),
            crate::xmp::PublishOutcome::Written
        );
        assert!(abs.is_file(), "写出后盘上应有文件：{abs:?}");

        // 清空 ⇒ 内容全空 ⇒ 删除自己的文件（「没有就没有」）。
        develop::clear(&conn, asset_id).unwrap();
        let asset = collect(&conn, asset_id).unwrap().unwrap();
        assert!(compose(&asset.content, 41, "RayBend test").is_none());
        assert!(crate::xmp::remove_if_ours(&abs).unwrap());
        assert!(!abs.exists());
    }
}
