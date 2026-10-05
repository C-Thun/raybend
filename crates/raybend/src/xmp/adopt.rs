//! 新资产的 sidecar 采纳。所有 catalog 写入与「仍为空」守卫在同一事务；文件 IO / 词典解析在外面。

use rusqlite::{Connection, OptionalExtension};

use super::ParsedSidecar;
use crate::store::{assets, develop, issues, tags, photo_tags};

/// 调用方仅传新登记的资产。关键词已经在 app.db 对齐成 ID。
/// `false` 表示资产不存在、已有用户数据，或 sidecar 没有可采纳内容。
pub fn adopt(
    conn: &Connection,
    asset_id: i64,
    parsed: &ParsedSidecar,
    tag_ids: &[i64],
    now_ms: i64,
) -> crate::Result<bool> {
    // 未知来源扩展不能把 subject 中的 AI 静默变成手动标签。
    if parsed.warnings.iter().any(|warning| warning.contains("rb:tags")) { return Ok(false); }
    let tx = conn.unchecked_transaction()?;
    let clean = tx
        .query_row(
            "SELECT rating = 0 AND color_label IS NULL AND like_state IS NULL AND lock_level = 0
         AND author IS NULL AND description IS NULL AND country IS NULL
         AND province_state IS NULL AND city IS NULL AND sublocation IS NULL
         FROM assets WHERE id = ?1",
            [asset_id],
            |row| row.get::<_, bool>(0),
        )
        .optional()?
        .unwrap_or(false);
    let latest = develop::load(&tx, asset_id)?;
    if !clean
        || !latest.is_empty()
        || latest.source_base != develop::EditBase::Raw
        || latest.auto_adjust.is_some()
        || !issues::list(&tx, asset_id)?.is_empty()
        || !tags::tags_of_asset(&tx, asset_id)?.is_empty()
        || photo_tags::snapshot(&tx, asset_id)?.has_content()
    {
        return Ok(false);
    }
    let mut adopted = false;
    if let Some(stack) = &parsed.latest {
        // 共用完整校验；坏编辑不挡其余可恢复元数据。
        if stack.validate().is_ok() {
            develop::save(&tx, asset_id, stack, now_ms)?;
            adopted = true;
        }
    }
    for profile in &parsed.profiles {
        if profile.stack.validate().is_err() {
            continue;
        }
        match issues::import_issue(
            &tx,
            asset_id,
            &profile.name,
            profile.source_base,
            profile.created_at_ms,
            profile.ordinal,
            &profile.stack,
        ) {
            Ok(created) => adopted |= created,
            Err(crate::Error::Unsupported(reason)) => {
                eprintln!("[sidecar] 定稿「{}」跳过：{reason}", profile.name)
            }
            Err(error) => return Err(error),
        }
    }
    let meta = &parsed.metadata;
    if !meta.is_empty() {
        assets::apply_sidecar_metadata(
            &tx,
            asset_id,
            meta.rating,
            meta.color_label.as_deref(),
            meta.author.as_deref(),
            meta.description.as_deref(),
            meta.country.as_deref(),
            meta.province_state.as_deref(),
            meta.city.as_deref(),
            meta.sublocation.as_deref(),
            now_ms,
        )?;
        adopted = true;
    }
    if let Some(state) = &parsed.tag_state {
        photo_tags::restore_sidecar(&tx, asset_id, state, now_ms)?;
        adopted |= state.has_content();
    } else {
        for id in tag_ids { adopted |= tags::attach_tag(&tx, asset_id, *id, now_ms)?; }
    }
    tx.commit()?;
    Ok(adopted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::develop::{DevelopStack, EditBase};
    use crate::store::migration::{self, DbKind};
    use crate::xmp::{SidecarContent, SidecarMetadata, SidecarProfile, compose, parse};

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::store::pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), 1).unwrap();
        conn.execute_batch("INSERT INTO assets(id, imported_at, updated_at) VALUES(1,1,1);")
            .unwrap();
        conn
    }

    fn parsed() -> ParsedSidecar {
        let mut latest = DevelopStack {
            source_base: EditBase::Sooc,
            ..Default::default()
        };
        latest.params.insert("exposure".into(), 0.5);
        let content = SidecarContent {
            latest: Some(latest.clone()),
            profiles: vec![SidecarProfile {
                name: "暖调".into(),
                source_base: EditBase::Sooc,
                created_at_ms: 42,
                ordinal: Some(7),
                stack: latest,
            }],
            metadata: SidecarMetadata {
                rating: 4,
                description: Some("恢复湖边说明\n第二行".into()),
                keywords: vec!["旅行".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        parse(&compose(&content, 0, "t").unwrap()).unwrap().unwrap()
    }

    #[test]
    fn recovery_is_complete_and_idempotent() {
        let conn = db();
        let sidecar = parsed();
        crate::store::fts::rebuild(&conn).unwrap();
        assert!(adopt(&conn, 1, &sidecar, &[7], 100).unwrap());
        assert_eq!(develop::load(&conn, 1).unwrap(), sidecar.latest.unwrap());
        let issue = issues::list(&conn, 1).unwrap().remove(0);
        assert_eq!(
            (issue.name.as_str(), issue.created_at, issue.ordinal),
            ("暖调", 42, 7)
        );
        assert_eq!(tags::tags_of_asset(&conn, 1).unwrap(), [7]);
        assert_eq!(
            conn.query_row("SELECT rating FROM assets WHERE id=1", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM assets_fts WHERE assets_fts MATCH '湖边说明'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert!(!adopt(&conn, 1, &parsed(), &[7], 101).unwrap());
        assert_eq!(issues::list(&conn, 1).unwrap().len(), 1);
    }

    #[test]
    fn existing_user_metadata_or_tags_prevent_overwriting() {
        for sql in [
            "UPDATE assets SET rating=2 WHERE id=1",
            "UPDATE assets SET description='人类新写的说明' WHERE id=1",
            "UPDATE assets SET color_label='red' WHERE id=1",
            "INSERT INTO asset_tags(asset_id,tag_id,tagged_at) VALUES(1,8,2)",
        ] {
            let conn = db();
            conn.execute_batch(sql).unwrap();
            assert!(!adopt(&conn, 1, &parsed(), &[7], 100).unwrap());
            assert!(develop::load(&conn, 1).unwrap().is_empty());
            assert!(issues::list(&conn, 1).unwrap().is_empty());
        }
    }

    #[test]
    fn empty_sooc_and_automatic_working_copies_are_not_clean_assets() {
        for latest in [
            DevelopStack {
                source_base: EditBase::Sooc,
                ..Default::default()
            },
            DevelopStack {
                auto_adjust: Some(Default::default()),
                ..Default::default()
            },
        ] {
            let conn = db();
            develop::save(&conn, 1, &latest, 2).unwrap();
            assert!(!adopt(&conn, 1, &parsed(), &[7], 100).unwrap());
            assert_eq!(develop::load(&conn, 1).unwrap(), latest);
        }
    }

    #[test]
    fn a_database_failure_rolls_back_the_entire_recovery() {
        let conn = db();
        conn.execute_batch("CREATE TRIGGER reject_tag BEFORE INSERT ON asset_tags BEGIN SELECT RAISE(ABORT, 'simulated write failure'); END;").unwrap();
        assert!(adopt(&conn, 1, &parsed(), &[7], 100).is_err());
        assert!(develop::load(&conn, 1).unwrap().is_empty());
        assert!(issues::list(&conn, 1).unwrap().is_empty());
        assert!(tags::tags_of_asset(&conn, 1).unwrap().is_empty());
        assert_eq!(
            conn.query_row("SELECT rating FROM assets WHERE id=1", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn invalid_edits_still_allow_valid_metadata_to_recover() {
        let conn = db();
        let mut sidecar = parsed();
        sidecar.latest.as_mut().unwrap().lut_enabled = Some(true);
        sidecar.profiles[0].stack.lut_enabled = Some(true);
        assert!(adopt(&conn, 1, &sidecar, &[7], 100).unwrap());
        assert!(develop::load(&conn, 1).unwrap().is_empty());
        assert!(issues::list(&conn, 1).unwrap().is_empty());
        assert_eq!(tags::tags_of_asset(&conn, 1).unwrap(), [7]);
    }

    #[test]
    fn absent_asset_and_empty_sidecar_are_noops() {
        let conn = db();
        assert!(!adopt(&conn, 9, &parsed(), &[7], 100).unwrap());
        assert!(!adopt(&conn, 1, &ParsedSidecar::default(), &[], 100).unwrap());
    }
}

#[cfg(test)]
mod tag_source_tests {
    use super::*;
    use crate::store::{migration::{self,DbKind}, photo_tags::{self,AiResult,AiOrigin,AiEvidence}};
    use crate::xmp::{SidecarContent, SidecarMetadata, compose_checked,parse,publish,PublishOutcome};
    fn db() -> Connection {
        let mut conn=Connection::open_in_memory().unwrap();
        crate::store::pragma::apply(&conn,false).unwrap();
        migration::apply(&mut conn,DbKind::Catalog,migration::Backups::none(),1).unwrap();
        conn.execute_batch("INSERT INTO assets(id,organization_uid,imported_at,updated_at) VALUES(1,'uid',1,1);").unwrap();conn
    }
    fn content() -> SidecarContent {
        let conn=db(); photo_tags::retain_manual(&conn,1,"旅行 & <山>",1).unwrap();
        let token=photo_tags::begin_attempt(&conn,1,"uid","original").unwrap();
        let result=AiResult {source_key:"original".into(),model_sha256:"a".repeat(64),pipeline_sha256:"b".repeat(64),generated_at:2,origin:AiOrigin::Local,valid:true,
            evidence:vec![AiEvidence {concept_key:"sea".into(),tag_name:"海".into(),score:0.3,accepted:true}]};
        photo_tags::commit_ai(&conn,1,&token,"original",&result,2).unwrap();
        photo_tags::mask(&conn,1,"海",true,3).unwrap();
        SidecarContent {tag_state:Some(photo_tags::snapshot(&conn,1).unwrap()),metadata:SidecarMetadata {keywords:photo_tags::effective_names(&conn,1).unwrap(),..Default::default()},..Default::default()}
    }
    #[test]
    fn extended_round_trip_keeps_masks_and_sources_and_standard_subject_is_effective() {
        let content=content();let xml=compose_checked(&content,0,"test").unwrap().unwrap();
        let parsed=parse(&xml).unwrap().unwrap();
        assert!(parsed.warnings.is_empty());assert_eq!(parsed.tag_state,content.tag_state);
        assert_eq!(parsed.metadata.keywords,["旅行 & <山>"]);
        let conn=db();assert!(adopt(&conn,1,&parsed,&[7,8],5).unwrap());
        let state=photo_tags::snapshot(&conn,1).unwrap();
        assert_eq!(state.manual,["旅行 & <山>"]);assert_eq!(state.ai,["海"]);assert_eq!(state.masks,["海"]);
        assert_eq!(state.result.unwrap().origin,AiOrigin::Sidecar);
        assert!(tags::tags_of_asset(&conn,1).unwrap().is_empty());
        assert!(!adopt(&conn,1,&parsed,&[],6).unwrap());
    }
    #[test]
    fn masks_only_and_empty_success_are_not_deleted() {
        let conn=db();photo_tags::mask(&conn,1,"鸟",true,1).unwrap();
        let c=SidecarContent{tag_state:Some(photo_tags::snapshot(&conn,1).unwrap()),..Default::default()};
        let parsed=parse(&compose_checked(&c,0,"test").unwrap().unwrap()).unwrap().unwrap();
        let other=db();assert!(adopt(&other,1,&parsed,&[],2).unwrap());
        assert_eq!(photo_tags::snapshot(&other,1).unwrap().masks,["鸟"]);
        let mut c=content();c.tag_state.as_mut().unwrap().version=2;assert!(compose_checked(&c,0,"test").is_err());
    }
    #[test]
    fn unknown_or_bad_tag_extension_is_preserved_and_not_imported_as_manual() {
        let xml=compose_checked(&content(),0,"test").unwrap().unwrap();
        for broken in [xml.replace("&quot;version&quot;:1","&quot;version&quot;:2"),xml.replace("&quot;score&quot;:0.3","&quot;score&quot;:9.0")] {
            assert_ne!(broken,xml);
            let parsed=parse(&broken).unwrap().unwrap();assert!(!parsed.warnings.is_empty());
            let conn=db();assert!(!adopt(&conn,1,&parsed,&[7],1).unwrap());
            let dir=tempfile::tempdir().unwrap();let path=dir.path().join("中文.xmp");std::fs::write(&path,&broken).unwrap();
            assert_eq!(publish(&path,&xml).unwrap(),PublishOutcome::Preserved);assert_eq!(std::fs::read_to_string(path).unwrap(),broken);
        }
    }
}
