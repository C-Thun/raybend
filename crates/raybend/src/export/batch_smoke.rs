//! Explicit tiny synthetic batch; excluded from seconds-level daily unit tests.
#[test]
#[ignore = "显式 1/1000 张小合成照片冒烟，不代表真实 RAW 性能"]
fn one_and_thousand() {
    use super::{
        Preset, SizeMode, VariantRef, VariantSnapshot,
        jobs::{Engine, Item, Outcome},
        metadata::Metadata,
    };
    use sha2::{Digest, Sha256};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    for count in [1usize, 1000] {
        let dir = tempfile::tempdir().unwrap();
        let sources = dir.path().join("source");
        let destination = dir.path().join("output");
        std::fs::create_dir_all(&sources).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let mut items = Vec::new();
        let mut hashes = Vec::new();
        for id in 1..=count {
            let source = sources.join(format!("照片{id}.png"));
            let image = crate::display::output::Rgb16Image::from_fn(8, 6, |x, y| {
                image::Rgb([((id + x as usize + y as usize) * 31) as u16, 12345, 54321])
            });
            image.save(&source).unwrap();
            hashes.push((
                source.clone(),
                Sha256::digest(std::fs::read(&source).unwrap()),
            ));
            let stack = crate::store::develop::DevelopStack {
                source_base: crate::store::develop::EditBase::Sooc,
                ..Default::default()
            };
            let snapshot = VariantSnapshot {
                reference: VariantRef {
                    asset_id: id as i64,
                    variant: "sooc".into(),
                },
                name: "SOOC".into(),
                rel_path: format!("照片{id}.png"),
                profile_hash: crate::store::issues::profile_hash(&stack).unwrap(),
                stack,
                source_signature: crate::media::source::source_signature(&source).unwrap(),
            };
            let preset = Preset {
                id: format!("preset{}", id % 4),
                name: format!("预设{}", id % 4),
                format: "png".into(),
                quality: 90,
                max_edge: 0,
                size_mode: SizeMode::Original,
                percent: 100,
                directory: destination.to_string_lossy().into_owned(),
                template: ":FILENAME".into(),
                existing_file: crate::export::ExistingFile::Append,
            };
            items.push(Item {
                id: String::new(),
                repository_id: "smoke".into(),
                root: sources.to_string_lossy().into_owned(),
                snapshot,
                preset,
                status: "pending".into(),
                error: None,
                sequence: 0,
                output: None,
            });
        }
        let injected = Arc::new(AtomicBool::new(count > 1));
        let fail = injected.clone();
        let engine = Engine::new(
            move |item| {
                if item.snapshot.reference.asset_id == 1 && fail.swap(false, Ordering::AcqRel) {
                    return Err("模拟磁盘写满".into());
                }
                let source = std::path::Path::new(&item.root).join(&item.snapshot.rel_path);
                super::output::execute(
                    &source,
                    &item.snapshot,
                    &item.preset,
                    &Default::default(),
                    &Metadata::default(),
                    item.sequence,
                    None,
                    None,
                )
                .map(|result| match result {
                    super::output::Publication::Written(path) => {
                        Outcome::Done(path.to_string_lossy().into_owned())
                    }
                    super::output::Publication::Skipped(path) => {
                        Outcome::FileExists(path.to_string_lossy().into_owned())
                    }
                })
                .map_err(|error| error.to_string())
            },
            |_| {},
        );
        for chunk in items.chunks(super::MAX_BATCH) {
            engine.enqueue(0, chunk.to_vec()).unwrap();
        }
        // Duplicate enqueue is idempotent even across batches.
        engine.enqueue(0, items[..1].to_vec()).unwrap();
        assert_eq!(
            engine.view().queues.values().map(Vec::len).sum::<usize>(),
            count
        );
        let began = std::time::Instant::now();
        for preset in engine.view().queues.keys().cloned().collect::<Vec<_>>() {
            engine.enable(preset, true).unwrap();
        }
        let deadline = began + std::time::Duration::from_secs(60);
        loop {
            let view = engine.view();
            if view
                .queues
                .values()
                .flatten()
                .all(|item| matches!(item.status.as_str(), "done" | "failed"))
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let failed = engine
            .view()
            .queues
            .values()
            .flatten()
            .filter(|item| item.status == "failed")
            .map(|item| item.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(failed.len(), usize::from(count > 1));
        if !failed.is_empty() {
            engine.retry(&failed).unwrap();
            loop {
                if engine
                    .view()
                    .queues
                    .values()
                    .flatten()
                    .all(|item| item.status == "done")
                {
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        assert_eq!(std::fs::read_dir(&destination).unwrap().count(), count);
        for (source, hash) in hashes {
            assert_eq!(hash, Sha256::digest(std::fs::read(source).unwrap()));
        }
        let rss = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|text| {
                text.lines()
                    .find(|line| line.starts_with("VmHWM:"))
                    .map(str::to_owned)
            });
        println!(
            "M4_BATCH count={count} size=8x6 format=RGB16_PNG presets=4 elapsed_ms={} failures={} retry=ok source_sha256=unchanged memory={rss:?}",
            began.elapsed().as_millis(),
            failed.len()
        );
        engine.stop_all().unwrap();
    }
}
