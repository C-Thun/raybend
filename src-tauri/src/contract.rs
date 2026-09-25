//! IPC 契约测试：Rust 侧序列化出来的键名，必须与前端 `src/api/types.ts` 的镜像一致。
//!
//! 这条测试存在的理由（`plans/M1-5.md` §3.1）：手写 TS 镜像最大的风险是**静默漂移** ——
//! Rust 改个字段名（或忘了 `rename_all = "camelCase"`），前端就永远拿到 `undefined`，
//! 界面表现为「某块一直空着」，既不报错也不崩，很难发现。
//!
//! 做法：两侧都对着**同一份** `src/api/dto-contract.json` 断言 ——
//! * 这里比对**真实 `serde_json` 序列化**出来的键集合；
//! * `src/api/dto-contract.test.ts` 比对 TS 接口的键表。
//!
//! 于是改字段名会同时惊动三处（Rust 结构 / JSON / TS 接口），漏不掉。
//!
//! ⚠️ 本文件只在 `cargo test` 下编译（`lib.rs` 里用 `#[cfg(test)]` 挂进来）。

use serde::Serialize;

use crate::import::{ImportPrecheckDto, ImportStartDto, InterruptedRunDto, PlannedRunDto};
use crate::repo::{RepositoryPathDto, RepositoryProbeDto, RepositoryViewDto};
use crate::repo::{RepositorySettingsDto, TemplatePreviewDto};
use crate::source::{
    DirEntryView, FileExifView, MetaFileView, PhotoCountView, PhotoMetaView, RecentDirView,
    SourceItemView, SourceScanView, TimeEntryView, VolumeView,
};

/// 前端契约文件（与 TS 侧共用同一份）。
const CONTRACT_JSON: &str = include_str!("../../src/api/dto-contract.json");

/// 一个响应结构的**真实**键集合（排序后）。
fn keys_of<T: Serialize + Default>() -> Vec<String> {
    let value = serde_json::to_value(T::default()).expect("响应结构必须能序列化");
    let object = value.as_object().expect("响应结构必须序列化成对象");
    let mut keys: Vec<String> = object.keys().cloned().collect();
    keys.sort();
    keys
}

/// 从一个**真实构造出来的值**取键集合（有的 DTO 没有 `Default`）。
fn keys_of_value<T: Serialize>(value: &T) -> Vec<String> {
    let value = serde_json::to_value(value).expect("响应结构必须能序列化");
    let object = value.as_object().expect("响应结构必须序列化成对象");
    let mut keys: Vec<String> = object.keys().cloned().collect();
    keys.sort();
    keys
}

/// 契约文件里某个 DTO 的键集合（排序后）。
fn contract_keys(name: &str) -> Vec<String> {
    let value: serde_json::Value =
        serde_json::from_str(CONTRACT_JSON).expect("契约文件必须是合法 JSON");
    let entry = value
        .get(name)
        .unwrap_or_else(|| panic!("契约文件里缺少 {name}"));
    let mut keys: Vec<String> = entry
        .as_array()
        .unwrap_or_else(|| panic!("{name} 必须是字符串数组"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("{name} 的元素必须是字符串"))
                .to_string()
        })
        .collect();
    keys.sort();
    keys
}

/// `include_str!` 的路径在编译期就定死 —— 文件被挪走会直接编译不过（这是有意的）。
#[test]
fn contract_file_is_readable() {
    let value: serde_json::Value = serde_json::from_str(CONTRACT_JSON).unwrap();
    assert!(value.is_object());
    assert!(value.get("RepositoryView").is_some());
}

// ── 导入（M1-6）──────────────────────────────────────────────
//
// 进度快照是**事件载荷**，比命令返回值更容易漂（没有返回值类型提示），
// 所以它也在契约里：字段名一改，前端拿到的就是 undefined，界面整块空着。

#[test]
fn import_batch_progress_keys_match_contract() {
    use raybend::import::progress::{BatchProgress, CurrentItem, ImportError, RunProgress};

    let mut progress = BatchProgress::new("b1", 1);
    let mut run = RunProgress::new(7, "/src");
    run.current = Some(CurrentItem {
        source: "a.jpg".to_string(),
        target: None,
    });
    progress.runs.push(run);
    progress.push_error(ImportError {
        source: "b.jpg".to_string(),
        target: None,
        reason: "读不了".to_string(),
        status: "failed".to_string(),
    });
    progress.recompute();

    assert_eq!(
        keys_of_value(&progress),
        contract_keys("ImportBatchProgress")
    );
    assert_eq!(
        keys_of_value(&progress.runs[0]),
        contract_keys("ImportRunProgress")
    );
    assert_eq!(
        keys_of_value(progress.runs[0].current.as_ref().unwrap()),
        contract_keys("ImportCurrentItem")
    );
    assert_eq!(
        keys_of_value(&progress.errors[0]),
        contract_keys("ImportError")
    );
}

#[test]
fn import_precheck_keys_match_contract() {
    let dto = ImportPrecheckDto {
        total_bytes: 1,
        free_bytes: Some(2),
        tight: false,
        needed_bytes: 3,
    };
    assert_eq!(keys_of_value(&dto), contract_keys("ImportPrecheck"));
}

#[test]
fn import_start_keys_match_contract() {
    let dto = ImportStartDto {
        batch_id: "b1".to_string(),
        runs: vec![PlannedRunDto {
            index: 0,
            source_root: "/src".to_string(),
        }],
    };
    assert_eq!(keys_of_value(&dto), contract_keys("ImportStart"));
    assert_eq!(
        keys_of_value(&dto.runs[0]),
        contract_keys("ImportPlannedRun")
    );
}

#[test]
fn meta_file_keys_match_contract() {
    let input = MetaFileView {
        relative: "a.jpg".to_string(),
        file_size: 10,
        mtime_ms: 20,
    };
    assert_eq!(keys_of_value(&input), contract_keys("MetaFile"));
}

#[test]
fn photo_meta_keys_match_contract() {
    let meta = PhotoMetaView {
        relative: "a.jpg".to_string(),
        width: 4000,
        height: 3000,
        orientation: 1,
    };
    assert_eq!(keys_of_value(&meta), contract_keys("PhotoMeta"));
}

#[test]
fn interrupted_run_keys_match_contract() {
    let dto = InterruptedRunDto {
        run_id: 1,
        source_root: "/src".to_string(),
        template: ":FILENAME".to_string(),
        started_at: 2,
        imported: 3,
        skipped: 4,
        failed: 5,
    };
    assert_eq!(keys_of_value(&dto), contract_keys("InterruptedRun"));
}

#[test]
fn repository_settings_keys_match_contract() {
    let dto = RepositorySettingsDto {
        repository_id: "r".to_string(),
        import_template: ":FILENAME".to_string(),
    };
    assert_eq!(keys_of_value(&dto), contract_keys("RepositorySettings"));
}

#[test]
fn template_preview_keys_match_contract() {
    let dto = TemplatePreviewDto {
        ok: true,
        error: None,
        warnings: vec![],
        paths: vec![],
    };
    assert_eq!(keys_of_value(&dto), contract_keys("TemplatePreview"));
}

// 每条断言都是「Rust 真实序列化 vs 契约文件」，逐个列出（函数名即断言名）。
#[test]
fn recent_dir_keys_match_contract() {
    assert_eq!(keys_of::<RecentDirView>(), contract_keys("RecentDir"));
}

#[test]
fn volume_keys_match_contract() {
    assert_eq!(keys_of::<VolumeView>(), contract_keys("Volume"));
}

#[test]
fn dir_entry_keys_match_contract() {
    assert_eq!(keys_of::<DirEntryView>(), contract_keys("DirEntry"));
}

#[test]
fn source_item_keys_match_contract() {
    assert_eq!(keys_of::<SourceItemView>(), contract_keys("SourceItem"));
}

#[test]
fn source_scan_keys_match_contract() {
    assert_eq!(keys_of::<SourceScanView>(), contract_keys("SourceScan"));
}

#[test]
fn time_entry_keys_match_contract() {
    assert_eq!(keys_of::<TimeEntryView>(), contract_keys("TimeEntry"));
}

#[test]
fn photo_count_keys_match_contract() {
    assert_eq!(keys_of::<PhotoCountView>(), contract_keys("PhotoCount"));
}

#[test]
fn file_exif_keys_match_contract() {
    assert_eq!(keys_of::<FileExifView>(), contract_keys("FileExif"));
}

#[test]
fn repository_view_keys_match_contract() {
    assert_eq!(
        keys_of::<RepositoryViewDto>(),
        contract_keys("RepositoryView")
    );
}

#[test]
fn repository_path_keys_match_contract() {
    assert_eq!(
        keys_of::<RepositoryPathDto>(),
        contract_keys("RepositoryPath")
    );
}

#[test]
fn repository_probe_keys_match_contract() {
    assert_eq!(
        keys_of::<RepositoryProbeDto>(),
        contract_keys("RepositoryProbe")
    );
}

#[test]
fn thumb_cache_stats_keys_match_contract() {
    assert_eq!(
        keys_of::<raybend::thumbnail::CacheStats>(),
        contract_keys("ThumbCacheStats")
    );
}

/// 契约文件里的每一项都必须被测到（防止「加了 DTO 忘了加断言」）。
#[test]
fn every_contract_entry_has_a_test() {
    let value: serde_json::Value = serde_json::from_str(CONTRACT_JSON).unwrap();
    let tested: Vec<&str> = vec![
        "RecentDir",
        "Volume",
        "DirEntry",
        "SourceItem",
        "SourceScan",
        "TimeEntry",
        "PhotoCount",
        "FileExif",
        "RepositoryView",
        "RepositoryPath",
        "RepositoryProbe",
        "ThumbCacheStats",
        // 目录元信息（M1-10：宽高 + 方向）
        "MetaFile",
        "PhotoMeta",
        // 导入（M1-6）
        "ImportBatchProgress",
        "ImportRunProgress",
        "ImportCurrentItem",
        "ImportError",
        "ImportPrecheck",
        "ImportStart",
        "ImportPlannedRun",
        "InterruptedRun",
        "RepositorySettings",
        "TemplatePreview",
        // 浏览（M2-W1）
        "AssetItem",
        "BrowseWindow",
        "TimelineEntry",
        "BrowseTimeline",
        "FacetCount",
        "BrowseFacets",
        "MarkingItem",
        "MarkResult",
        "DeleteResult",
        "DeleteFailure",
        "FlagsView",
        "DirEmptyView",
        // 数据库升级通知（M2-W2）
        "MigrationNotice",
        // 重建数据（M2-W2 数量体系）
        "RebuildProgress",
        "RebuildReport",
        // 编辑视口（M3-W1 洞口契约 / M3-W2 渲染线程）
        "EditorViewportState",
        "EditorRenderState",
        "LensProfile",
        "LensMatch",
        // 全屏看图（M3 晚：清单与下标跨 IPC）
        "FullscreenItem",
        "FullscreenPayload",
    ];
    for key in value.as_object().unwrap().keys() {
        if key.starts_with('_') {
            continue; // `_comment` 之类的说明项
        }
        assert!(
            tested.contains(&key.as_str()),
            "契约文件里的 {key} 没有对应的断言"
        );
    }
}

// ── 浏览（M2-W1）────────────────────────────────────────────
//
// 浏览的响应结构数量多、字段也多（一张照片的元信息 + 筛选分面 + 标记结果），
// 少写一个键就是「界面上永远空着」那类静默故障 —— 全部进契约。

#[test]
fn browse_assets_dtos_match_contract() {
    use crate::browse::{
        AssetItem, BrowseFacets, BrowseTimeline, BrowseWindow, DeleteResult, FacetCount, FlagsView,
        MarkResult, MarkingItem, TimelineEntry,
    };
    use crate::dirs::DirEmptyView;

    assert_eq!(keys_of::<AssetItem>(), contract_keys("AssetItem"));
    assert_eq!(keys_of::<BrowseWindow>(), contract_keys("BrowseWindow"));
    assert_eq!(keys_of::<TimelineEntry>(), contract_keys("TimelineEntry"));
    assert_eq!(keys_of::<BrowseTimeline>(), contract_keys("BrowseTimeline"));
    assert_eq!(keys_of::<FacetCount>(), contract_keys("FacetCount"));
    assert_eq!(keys_of::<BrowseFacets>(), contract_keys("BrowseFacets"));
    assert_eq!(keys_of::<MarkingItem>(), contract_keys("MarkingItem"));
    assert_eq!(keys_of::<MarkResult>(), contract_keys("MarkResult"));
    assert_eq!(keys_of::<DeleteResult>(), contract_keys("DeleteResult"));
    assert_eq!(keys_of::<FlagsView>(), contract_keys("FlagsView"));
    assert_eq!(keys_of::<DirEmptyView>(), contract_keys("DirEmptyView"));
}

/// 迁移通知也要进契约：字段名漂了，前端拿到 `undefined` 就撤不掉阻塞遮罩
/// （表现为「升级完界面还是死的」—— 正是这个桥要防的那类静默故障）。
#[test]
fn migration_notice_matches_contract() {
    use raybend::store::migration::{DbKind, MigrationNotice, MigrationPhase};

    let start = crate::migration::to_event(MigrationNotice {
        kind: DbKind::Catalog,
        from: 3,
        to: 4,
        phase: MigrationPhase::Start,
    });
    assert_eq!(keys_of_value(&start), contract_keys("MigrationNotice"));
}

/// 「重建数据」的结果也要进契约（字段名漂了，界面上那句摘要就是一堆 `undefined`）。
#[test]
fn rebuild_report_matches_contract() {
    let progress = crate::repo::RebuildProgressDto {
        repository_id: "r".to_string(),
        phase: "scan".to_string(),
        done: 1,
        total: 2,
    };
    assert_eq!(keys_of_value(&progress), contract_keys("RebuildProgress"));
    assert_eq!(
        keys_of::<crate::repo::RebuildReportDto>(),
        contract_keys("RebuildReport")
    );
}

#[test]
fn delete_failure_is_a_nested_dto_too() {
    use crate::browse::DeleteFailure;
    assert_eq!(keys_of::<DeleteFailure>(), contract_keys("DeleteFailure"));
}

/// 编辑视口的两块状态（M3）也要进契约。
///
/// 它们比别的 DTO 更容易漂：前端**同时**读 `EditorViewportState`（洞口回读）与
/// `EditorRenderState`（渲染线程快照 + 握手），而且后者里有几个字段是「界面据此决定
/// 要不要把洞口切成透明」的开关（`ready` / `paintedPath`）—— 名字一漂，
/// 症状是「照片永远不出现」或「洞口一直是 DOM 底色」，既不报错也不崩。
#[test]
fn editor_viewport_state_matches_contract() {
    use crate::editor::ViewportStateDto;
    assert_eq!(
        keys_of::<ViewportStateDto>(),
        contract_keys("EditorViewportState")
    );
}

#[test]
fn editor_render_state_matches_contract() {
    use crate::editor::RenderState;
    assert_eq!(keys_of::<RenderState>(), contract_keys("EditorRenderState"));
}

/// 全屏看图的清单（`fullscreen_open` 收它、`fullscreen_payload` 回它）也进契约。
///
/// 键名一漂的症状：页面拿到的 `items` 全是 `undefined`，或 Rust 侧反序列化失败 ——
/// 全屏窗开着但永远黑屏，不报错也不崩。
#[test]
fn fullscreen_payload_matches_contract() {
    use crate::fullscreen::{FullscreenItem, FullscreenPayload};
    assert_eq!(keys_of::<FullscreenItem>(), contract_keys("FullscreenItem"));
    assert_eq!(
        keys_of::<FullscreenPayload>(),
        contract_keys("FullscreenPayload")
    );
}

#[test]
fn lens_match_keys_match_contract() {
    use crate::lens::{LensMatchDto, LensProfileDto};
    let profile = LensProfileDto {
        key: "Panasonic|12-60".into(),
        maker: "Panasonic".into(),
        model: "12-60".into(),
        rectilinear: true,
        focal_min: 12.0,
        focal_max: 60.0,
    };
    assert_eq!(keys_of_value(&profile), contract_keys("LensProfile"));
    let response = LensMatchDto {
        ready: true,
        detected: Some(profile.clone()),
        lens_name: None,
        focal_mm: None,
        candidates: vec![profile],
        warnings: vec!["metadata unavailable".into()],
    };
    assert_eq!(keys_of_value(&response), contract_keys("LensMatch"));
}
