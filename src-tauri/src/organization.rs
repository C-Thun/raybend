//! 相片整理 IPC：薄壳只选择 app/catalog 租约并转发核心库操作。

use raybend::store::{db::AppDb, organization::{self, Bucket, PhotoRef, RuleSet}, query::{self, Query, Scope}, tags, time};
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::{browse::{AssetItem, BrowseState, BrowseTimeline, FilterDto, TimelineEntry}, db::DbState, source::blocking};

#[tauri::command]
pub async fn organization_buckets<R: Runtime>(app: AppHandle<R>) -> Result<Vec<Bucket>, String> {
    blocking(move || app.state::<DbState>().with(&app, |db| {
        db.read(organization::list_buckets).map_err(|e| e.to_string())
    })).await
}

#[tauri::command]
pub async fn organization_bucket_create<R: Runtime>(app: AppHandle<R>, name: String, rules: RuleSet) -> Result<Bucket, String> {
    blocking(move || app.state::<DbState>().with(&app, |db| {
        db.write(move |conn| organization::create_bucket(conn, &name, rules, time::now_millis())).map_err(|e| e.to_string())
    })).await
}

#[tauri::command]
pub async fn organization_bucket_rename<R: Runtime>(app: AppHandle<R>, id: i64, name: String) -> Result<bool, String> {
    blocking(move || app.state::<DbState>().with(&app, |db| {
        db.write(move |conn| organization::rename_bucket(conn, id, &name, time::now_millis())).map_err(|e| e.to_string())
    })).await
}

#[tauri::command]
pub async fn organization_bucket_rules<R: Runtime>(app: AppHandle<R>, id: i64, name: String, rules: RuleSet) -> Result<Option<Bucket>, String> {
    blocking(move || app.state::<DbState>().with(&app, |db| {
        db.write(move |conn| organization::set_bucket_rules(conn, id, &name, rules, time::now_millis())).map_err(|e| e.to_string())
    })).await
}

#[tauri::command]
pub async fn organization_bucket_pin<R: Runtime>(app: AppHandle<R>, id: i64, pinned: bool) -> Result<bool, String> {
    blocking(move || app.state::<DbState>().with(&app, |db| {
        db.write(move |conn| organization::set_bucket_pinned(conn, id, pinned, time::now_millis())).map_err(|e| e.to_string())
    })).await
}

#[tauri::command]
pub async fn organization_bucket_pause<R: Runtime>(app: AppHandle<R>, id: i64, paused: bool) -> Result<bool, String> {
    blocking(move || app.state::<DbState>().with(&app, |db| {
        db.write(move |conn| organization::set_bucket_paused(conn, id, paused, time::now_millis())).map_err(|e| e.to_string())
    })).await
}

#[tauri::command]
pub async fn organization_bucket_delete<R: Runtime>(app: AppHandle<R>, id: i64) -> Result<bool, String> {
    blocking(move || app.state::<DbState>().with(&app, |db| {
        db.write(move |conn| organization::delete_bucket(conn, id)).map_err(|e| e.to_string())
    })).await
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIdentity {
    repository_id: String,
    asset_id: i64,
}

fn resolve_refs<R: Runtime>(app: &AppHandle<R>, photos: &[AssetIdentity]) -> Result<Vec<PhotoRef>, String> {
    let state = app.state::<BrowseState>();
    let mut out = Vec::with_capacity(photos.len());
    let mut by_repository = std::collections::BTreeMap::<String, Vec<i64>>::new();
    for photo in photos {
        by_repository.entry(photo.repository_id.clone()).or_default().push(photo.asset_id);
    }
    for (repository_id, asset_ids) in by_repository {
        out.extend(state.with_catalog(app, &repository_id, |db| {
            db.read(|conn| organization::photo_refs(conn, &repository_id, &asset_ids))
                .map_err(|e| e.to_string())
        })?);
    }
    Ok(out)
}

#[tauri::command]
pub async fn organization_bucket_add<R: Runtime>(app: AppHandle<R>, bucket_id: i64, photos: Vec<AssetIdentity>) -> Result<usize, String> {
    blocking(move || {
        let refs = resolve_refs(&app, &photos)?;
        app.state::<DbState>().with(&app, |db| {
            db.write(move |conn| organization::add_members(conn, bucket_id, &refs, time::now_millis()))
                .map_err(|e| e.to_string())
        })
    }).await
}

#[tauri::command]
pub async fn organization_bucket_remove<R: Runtime>(app: AppHandle<R>, bucket_id: i64, photo: AssetIdentity) -> Result<bool, String> {
    blocking(move || {
        let mut refs = resolve_refs(&app, &[photo])?;
        let reference = refs.pop().expect("单张照片");
        app.state::<DbState>().with(&app, |db| {
            db.write(move |conn| organization::remove_member(conn, bucket_id, &reference, time::now_millis()))
                .map_err(|e| e.to_string())
        })
    }).await
}

#[tauri::command]
pub async fn organization_bucket_members<R: Runtime>(app: AppHandle<R>, bucket_id: i64, repository_id: String) -> Result<Vec<PhotoRef>, String> {
    blocking(move || {
        let refs = app.state::<DbState>().with(&app, |db| {
            db.read(|conn| organization::member_refs(conn, bucket_id, &repository_id)).map_err(|e| e.to_string())
        })?;
        app.state::<BrowseState>().with_catalog(&app, &repository_id, |db| {
            db.read(|conn| organization::valid_member_refs(conn, refs)).map_err(|e| e.to_string())
        })
    }).await
}

/// 标签面板进入时同步旧数字 ID 的文字。找不到词典名的旧记录保留在 asset_tags。
#[tauri::command]
pub async fn organization_tag_sync<R: Runtime>(app: AppHandle<R>, repository_id: String) -> Result<usize, String> {
    blocking(move || {
        let dictionary = app.state::<DbState>().with(&app, |db| db.read(tags::list_all).map_err(|e| e.to_string()))?;
        app.state::<BrowseState>().with_catalog(&app, &repository_id, |db| {
            db.write(move |conn| organization::sync_legacy_terms(conn, &dictionary)).map_err(|e| e.to_string())
        })
    }).await
}

#[tauri::command]
pub async fn organization_directory_tag<R: Runtime>(
    app: AppHandle<R>, repository_id: String, directory_key: String, name: String, enabled: bool,
) -> Result<bool, String> {
    blocking(move || {
        if enabled {
            let name_for_dictionary = name.clone();
            app.state::<DbState>().with(&app, |db| {
                db.write(move |conn| tags::ensure_tag(conn, &name_for_dictionary, time::now_millis())).map_err(|e| e.to_string())
            })?;
        }
        app.state::<BrowseState>().with_catalog(&app, &repository_id, |db| {
            db.write(move |conn| organization::set_directory_tag(conn, &directory_key, &name, enabled, time::now_millis()))
                .map_err(|e| e.to_string())
        })
    }).await
}

#[tauri::command]
pub async fn organization_tag_directory_counts<R: Runtime>(
    app: AppHandle<R>, repository_id: String,
) -> Result<std::collections::BTreeMap<String, i64>, String> {
    blocking(move || app.state::<BrowseState>().with_catalog(&app, &repository_id, |db| {
        db.read(organization::directory_tag_counts).map_err(|e| e.to_string())
    })).await
}

#[tauri::command]
pub async fn organization_tag_directories<R: Runtime>(
    app: AppHandle<R>, repository_id: String, tag_key: String, offset: usize, limit: usize,
) -> Result<Vec<String>, String> {
    blocking(move || app.state::<BrowseState>().with_catalog(&app, &repository_id, |db| {
        db.read(|conn| organization::directories_for_tag(conn, &tag_key, limit.min(500), offset))
            .map_err(|e| e.to_string())
    })).await
}

#[tauri::command]
pub async fn organization_directory_tags<R: Runtime>(
    app: AppHandle<R>, repository_id: String, directory_key: String,
) -> Result<Vec<String>, String> {
    blocking(move || app.state::<BrowseState>().with_catalog(&app, &repository_id, |db| {
        db.read(|conn| organization::tags_for_directory(conn, &directory_key))
            .map_err(|e| e.to_string())
    })).await
}

#[tauri::command]
pub async fn organization_tag_timeline<R: Runtime>(
    app: AppHandle<R>, repository_id: String, tag_key: String, filter: FilterDto,
) -> Result<BrowseTimeline, String> {
    blocking(move || app.state::<BrowseState>().with_catalog(&app, &repository_id, |db| {
        let q = Query {
            scope: Scope::TagKey { key: tags::fold_name(&tag_key) },
            filter: filter.into_filter(),
            sort: query::Sort::new(query::SortKey::TakenAt, true),
        };
        db.read(|conn| {
            let total = query::count(conn, &q)?;
            let entries = query::timeline(conn, &q, 0)?.into_iter().map(|row| TimelineEntry {
                id: row.id, taken_at: row.taken_at, rel_path: row.rel_path,
            }).collect();
            Ok(BrowseTimeline { total, entries })
        }).map_err(|e| e.to_string())
    })).await
}

#[tauri::command]
pub async fn organization_bucket_timeline<R: Runtime>(
    app: AppHandle<R>, bucket_id: i64, repository_id: String, filter: FilterDto,
) -> Result<BrowseTimeline, String> {
    blocking(move || {
        let refs = app.state::<DbState>().with(&app, |db| {
            db.read(|conn| organization::member_refs(conn, bucket_id, &repository_id)).map_err(|e| e.to_string())
        })?;
        app.state::<BrowseState>().with_catalog(&app, &repository_id, |db| {
            db.read(|conn| {
                let refs = organization::valid_member_refs(conn, refs)?;
                let mut entries = Vec::with_capacity(refs.len());
                for chunk in refs.chunks(400) {
                    let q = Query {
                        scope: Scope::AssetIds { ids: chunk.iter().map(|item| item.asset_id).collect() },
                        filter: filter.clone().into_filter(),
                        sort: query::Sort::new(query::SortKey::TakenAt, true),
                    };
                    entries.extend(query::timeline(conn, &q, 0)?.into_iter().map(|row| TimelineEntry {
                        id: row.id, taken_at: row.taken_at, rel_path: row.rel_path,
                    }));
                }
                entries.sort_by(|a, b| b.taken_at.cmp(&a.taken_at).then_with(|| a.id.cmp(&b.id)));
                Ok(BrowseTimeline { total: entries.len() as i64, entries })
            }).map_err(|e| e.to_string())
        })
    }).await
}

#[tauri::command]
pub async fn organization_assets<R: Runtime>(
    app: AppHandle<R>, repository_id: String, ids: Vec<i64>,
) -> Result<Vec<AssetItem>, String> {
    if ids.len() > 500 { return Err("单次最多读取 500 张照片".into()); }
    blocking(move || app.state::<BrowseState>().with_catalog(&app, &repository_id, |db| {
        let q = Query::new(Scope::AssetIds { ids });
        db.read(|conn| query::page(conn, &q, 0, 500))
            .map(|rows| rows.into_iter().map(AssetItem::from).collect())
            .map_err(|e| e.to_string())
    })).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileReport {
    pub processed_batches: usize,
    pub failed_repositories: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesPreview {
    pub matches: usize,
    pub failed_repositories: Vec<String>,
}

/// 完整规则预览。仅在弹窗用户操作后请求，不加入后台热路径。
#[tauri::command]
pub async fn organization_rules_preview<R: Runtime>(
    app: AppHandle<R>, rules: RuleSet,
) -> Result<RulesPreview, String> {
    blocking(move || {
        let rules = organization::normalize_rules(rules).map_err(|e| e.to_string())?;
        let repositories = rules.groups.iter().flat_map(|g| g.repository_ids.iter())
            .cloned().collect::<std::collections::BTreeSet<_>>();
        let mut result = RulesPreview { matches: 0, failed_repositories: Vec::new() };
        for id in repositories {
            let count = app.state::<BrowseState>().with_catalog(&app, &id, |db| {
                db.read(|conn| {
                    let mut matched = std::collections::BTreeSet::new();
                    for group in &rules.groups {
                        if !group.repository_ids.contains(&id) { continue; }
                        let q = Query {
                            scope: Scope::Repository,
                            filter: organization::filter_for_group(group),
                            sort: query::Sort::default(),
                        };
                        matched.extend(query::timeline(conn, &q, 0)?.into_iter().map(|row| row.id));
                    }
                    Ok(matched.len())
                }).map_err(|e| e.to_string())
            });
            match count {
                Ok(count) => result.matches += count,
                Err(_) => result.failed_repositories.push(id),
            }
        }
        Ok(result)
    }).await
}

/// 每次只做有限批次；调用方可在库变化后重复唤醒，游标跨重启保存。
#[tauri::command]
pub async fn organization_reconcile<R: Runtime>(
    app: AppHandle<R>, repository_ids: Vec<String>,
) -> Result<ReconcileReport, String> {
    blocking(move || {
        let mut report = ReconcileReport { processed_batches: 0, failed_repositories: Vec::new() };
        for id in repository_ids {
            let state = app.state::<BrowseState>();
            let outcome = (|| {
                let _permit = state.sessions.begin_task(&id).map_err(|e| e.to_string())?;
                let cat = state.lease(&app, &id)?;
                let repo = id.clone();
                let count = app.state::<DbState>().with(&app, |app_db: &AppDb| {
                    app_db.write(move |app_conn| cat.read(|cat_conn| {
                        let scan = organization::reconcile_scan_batch(app_conn, cat_conn, &repo, time::now_millis())?;
                        let events = organization::reconcile_events_batch(app_conn, cat_conn, &repo, time::now_millis())?;
                        Ok(usize::from(scan) + usize::from(events))
                    })).map_err(|e| e.to_string())
                })?;
                let cursor = app.state::<DbState>().with(&app, |app_db: &AppDb| {
                    app_db.read(|conn| organization::event_cursor(conn, &id)).map_err(|e| e.to_string())
                })?;
                if cursor > 0 {
                    state.with_catalog(&app, &id, |db| {
                        db.write(move |conn| organization::prune_events(conn, cursor)).map_err(|e| e.to_string())
                    })?;
                }
                Ok::<usize, String>(count)
            })();
            match outcome {
                Ok(count) => report.processed_batches += count,
                Err(_) => report.failed_repositories.push(id),
            }
        }
        Ok(report)
    }).await
}


#[tauri::command]
pub async fn organization_photo_tag_state<R:Runtime>(app:AppHandle<R>,repository_id:String,asset_id:i64)->Result<raybend::store::photo_tags::TagState,String> {
    blocking(move || {
        let dictionary=app.state::<DbState>().with(&app,|db|db.read(tags::list_all).map_err(|e|e.to_string()))?;
        let catalog=app.state::<BrowseState>().lease(&app,&repository_id)?;
        catalog.write(move|conn|organization::sync_legacy_terms(conn,&dictionary)).map_err(|e|e.to_string())?;
        catalog.read(move|conn|raybend::store::photo_tags::snapshot(conn,asset_id)).map_err(|e|e.to_string())
    }).await
}
