//! 一次只领取一张的调度器；应用适配器只负责租约/事件，业务流程在此。
use super::{
    jobs::{Claim, Run},
    pack::Verified,
    session::{Lease, Prepared},
};
use crate::{Error, Result};
use std::sync::Arc;
pub enum Availability {
    Offline,
    Photo(Prepared),
}
pub trait Host {
    fn foreground_busy(&self) -> bool;
    fn claim(&self) -> Result<Option<(Claim, Run)>>;
    fn model(&self, pipeline: &str) -> Result<Arc<Verified>>;
    fn prepare(&self, claim: &Claim, run: &Run) -> Result<Availability>;
    fn finish(
        &self,
        claim: &Claim,
        run: &Run,
        lease: Option<&Lease>,
        pack: &Verified,
        features: &[f32],
    ) -> Result<bool>;
    fn discard(&self, lease: &Lease) -> Result<()>;
    fn offline(&self, claim: &Claim) -> Result<()>;
    fn fail(&self, claim: &Claim, error: &str) -> Result<()>;
}
pub trait Engine {
    fn encode(&mut self, lease: &Lease, pack: &Verified) -> Result<Vec<f32>>;
    fn unload(&mut self);
}
pub fn step(host: &impl Host, engine: &mut impl Engine) -> Result<bool> {
    if host.foreground_busy() {
        return Ok(false);
    }
    let Some((claim, run)) = host.claim()? else {
        return Ok(false);
    };
    let mut lease = None;
    let result = (|| -> Result<()> {
        let pack = host.model(&run.pipeline_sha256)?;
        match host.prepare(&claim, &run)? {
            Availability::Offline => host.offline(&claim)?,
            Availability::Photo(Prepared::Unavailable(message)) => {
                return Err(Error::Unsupported(message));
            }
            Availability::Photo(Prepared::Known | Prepared::Locked | Prepared::Removed) => {
                host.finish(&claim, &run, None, &pack, &[])?;
            }
            Availability::Photo(Prepared::Ready(prepared)) => {
                lease = Some(prepared);
                let current = lease.as_ref().unwrap();
                let features = engine.encode(current, &pack)?;
                if !host.finish(&claim, &run, Some(current), &pack, &features)? {
                    return Err(Error::Unsupported(
                        "识别已取消或原片已改变，结果未写入".into(),
                    ));
                }
            }
        }
        Ok(())
    })();
    if let Some(lease) = lease {
        host.discard(&lease)?;
    }
    if let Err(error) = result {
        host.fail(&claim, &error.to_string())?;
    }
    Ok(true)
}
#[cfg(feature = "ai-runtime")]
pub struct RuntimeEngine {
    pub bin: std::path::PathBuf,
    pub library: std::path::PathBuf,
    shutdown: super::worker::Shutdown,
    current: Option<(std::path::PathBuf, super::worker::Worker)>,
}
#[cfg(feature = "ai-runtime")]
impl RuntimeEngine {
    pub fn new(bin: std::path::PathBuf, library: std::path::PathBuf) -> Self {
        Self::with_shutdown(bin, library, super::worker::Shutdown::default())
    }
    pub fn with_shutdown(
        bin: std::path::PathBuf,
        library: std::path::PathBuf,
        shutdown: super::worker::Shutdown,
    ) -> Self {
        Self {
            bin,
            library,
            shutdown,
            current: None,
        }
    }
}
#[cfg(feature = "ai-runtime")]
impl Engine for RuntimeEngine {
    fn encode(&mut self, lease: &Lease, pack: &Verified) -> Result<Vec<f32>> {
        let path = pack.directory.join("image_encoder.onnx");
        if self.current.as_ref().is_none_or(|(old, _)| old != &path) {
            self.unload();
            self.current = Some((
                path.clone(),
                super::worker::Worker::with_shutdown(
                    self.bin.clone(),
                    self.library.clone(),
                    path,
                    self.shutdown.clone(),
                ),
            ));
        }
        self.current
            .as_mut()
            .unwrap()
            .1
            .encode(
                &lease.path,
                lease.input.file.role == "raw",
                pack.manifest.resize,
            )
            .map_err(|e| Error::Unsupported(e.to_string()))
    }
    fn unload(&mut self) {
        self.current.take();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Busy {
        claims: Cell<usize>,
    }
    impl Host for Busy {
        fn foreground_busy(&self) -> bool {
            true
        }
        fn claim(&self) -> Result<Option<(Claim, Run)>> {
            self.claims.set(self.claims.get() + 1);
            Ok(None)
        }
        fn model(&self, _: &str) -> Result<Arc<Verified>> {
            unreachable!()
        }
        fn prepare(&self, _: &Claim, _: &Run) -> Result<Availability> {
            unreachable!()
        }
        fn finish(
            &self,
            _: &Claim,
            _: &Run,
            _: Option<&Lease>,
            _: &Verified,
            _: &[f32],
        ) -> Result<bool> {
            unreachable!()
        }
        fn discard(&self, _: &Lease) -> Result<()> {
            unreachable!()
        }
        fn offline(&self, _: &Claim) -> Result<()> {
            unreachable!()
        }
        fn fail(&self, _: &Claim, _: &str) -> Result<()> {
            unreachable!()
        }
    }
    struct NotCalled;
    impl Engine for NotCalled {
        fn encode(&mut self, _: &Lease, _: &Verified) -> Result<Vec<f32>> {
            unreachable!()
        }
        fn unload(&mut self) {}
    }
    #[test]
    fn foreground_work_stops_claiming_at_image_boundary() {
        let h = Busy {
            claims: Cell::new(0),
        };
        assert!(!step(&h, &mut NotCalled).unwrap());
        assert_eq!(h.claims.get(), 0);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::super::{coordinator::Coordinator, jobs, pack, session, status};
    use super::*;
    use crate::store::{db::AppDb, migration, organization::PhotoRef, photo_tags, time};
    use rusqlite::Connection;

    // 用真实 jobs/catalog/提交屏障，只替换昂贵模型；不解码假图片或运行 ONNX fixture。
    struct Harness {
        app: AppDb,
        catalog: Connection,
        root: tempfile::TempDir,
        pack: Arc<Verified>,
        pipeline: String,
        coordinator: Coordinator,
    }
    impl Harness {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let app = AppDb::open(&root.path().join("app"), 1).unwrap();
            let mut catalog = Connection::open_in_memory().unwrap();
            crate::store::pragma::apply(&catalog, true).unwrap();
            migration::apply(
                &mut catalog,
                migration::DbKind::Catalog,
                migration::Backups::none(),
                1,
            )
            .unwrap();
            catalog.execute_batch("INSERT INTO assets(id,organization_uid,imported_at,updated_at) VALUES(1,'uid',1,1);INSERT INTO asset_files(id,asset_id,rel_path,rel_path_folded,role,ext,created_at,updated_at) VALUES(1,1,'原片.JPG','原片.jpg','bitmap','jpg',1,1)").unwrap();
            std::fs::write(root.path().join("原片.JPG"), b"original signature fixture").unwrap();
            let model_dir = root.path().join("model");
            let pipeline = pack::tests::pack(&model_dir, true);
            let pack = Arc::new(pack::verify(&model_dir, &pipeline, true).unwrap());
            Self {
                app,
                catalog,
                root,
                pack,
                pipeline,
                coordinator: Coordinator::default(),
            }
        }
        fn run(&self, rerun: bool) -> i64 {
            let pipeline = self.pipeline.clone();
            self.app
                .write(move |c| {
                    let id = jobs::create_with_options(
                        c,
                        "测试识别",
                        &pipeline,
                        rerun,
                        true,
                        time::now_millis(),
                    )?;
                    jobs::append_page(
                        c,
                        id,
                        &[PhotoRef {
                            repository_id: "库".into(),
                            asset_id: 1,
                            photo_uid: "uid".into(),
                        }],
                        time::now_millis(),
                    )?;
                    jobs::finish_prepare(c, id, time::now_millis())?;
                    Ok(id)
                })
                .unwrap()
        }
        fn task(&self, id: i64) -> status::Task {
            self.app
                .read(status::tasks)
                .unwrap()
                .into_iter()
                .find(|t| t.id == id)
                .unwrap()
        }
    }
    impl Host for Harness {
        fn foreground_busy(&self) -> bool {
            false
        }
        fn claim(&self) -> Result<Option<(Claim, Run)>> {
            self.app.write(|c| {
                let Some(claim) = jobs::claim_next(c, time::now_millis())? else {
                    return Ok(None);
                };
                let run = jobs::context(c, claim.item.run_id)?;
                Ok(Some((claim, run)))
            })
        }
        fn model(&self, _: &str) -> Result<Arc<Verified>> {
            Ok(self.pack.clone())
        }
        fn prepare(&self, claim: &Claim, run: &Run) -> Result<Availability> {
            let tx = self.catalog.unchecked_transaction()?;
            let value = session::prepare(
                &tx,
                self.root.path(),
                claim.item.photo.clone(),
                &run.pipeline_sha256,
                run.rerun,
                time::now_millis(),
            )?;
            tx.commit()?;
            Ok(Availability::Photo(value))
        }
        fn finish(
            &self,
            claim: &Claim,
            run: &Run,
            lease: Option<&Lease>,
            pack: &Verified,
            features: &[f32],
        ) -> Result<bool> {
            self.coordinator
                .finish_as(&self.app, claim, lease.is_none(), || {
                    let Some(lease) = lease else { return Ok(true) };
                    let tx = self.catalog.unchecked_transaction()?;
                    let committed = session::commit(
                        &tx,
                        self.root.path(),
                        lease,
                        pack,
                        features,
                        &run.pipeline_sha256,
                        run.zh,
                        time::now_millis(),
                    )?;
                    tx.commit()?;
                    Ok(committed)
                })
        }
        fn discard(&self, lease: &Lease) -> Result<()> {
            session::discard(&self.catalog, lease).map(|_| ())
        }
        fn offline(&self, claim: &Claim) -> Result<()> {
            let claim = claim.clone();
            self.app
                .write(move |c| jobs::wait_offline(c, &claim, time::now_millis()))
        }
        fn fail(&self, claim: &Claim, error: &str) -> Result<()> {
            let claim = claim.clone();
            let error = error.to_owned();
            self.app
                .write(move |c| jobs::fail(c, &claim, &error, time::now_millis()).map(|_| ()))
        }
    }
    struct FixedEngine<'a> {
        calls: usize,
        fail: bool,
        before: Option<Box<dyn FnMut() + 'a>>,
    }
    impl Engine for FixedEngine<'_> {
        fn encode(&mut self, _: &Lease, _: &Verified) -> Result<Vec<f32>> {
            self.calls += 1;
            if let Some(hook) = self.before.as_mut() {
                hook();
            }
            if self.fail {
                Err(Error::Unsupported("synthetic inference failure".into()))
            } else {
                Ok(vec![1.; 512])
            }
        }
        fn unload(&mut self) {}
    }
    #[test]
    fn real_queue_commit_skip_and_failed_rerun_keep_the_previous_success() {
        let h = Harness::new();
        let mut engine = FixedEngine {
            calls: 0,
            fail: false,
            before: None,
        };
        let first = h.run(false);
        assert!(step(&h, &mut engine).unwrap());
        assert_eq!(h.task(first).done, 1);
        assert_eq!(photo_tags::effective_names(&h.catalog, 1).unwrap(), ["鸟"]);
        let previous = photo_tags::snapshot(&h.catalog, 1).unwrap().result.unwrap();
        let repeated = h.run(false);
        assert!(step(&h, &mut engine).unwrap());
        assert_eq!(h.task(repeated).skipped, 1);
        assert_eq!(engine.calls, 1);
        h.run(true);
        engine.fail = true;
        assert!(step(&h, &mut engine).unwrap());
        assert_eq!(
            photo_tags::snapshot(&h.catalog, 1).unwrap().result.unwrap(),
            previous
        );
        assert_eq!(photo_tags::effective_names(&h.catalog, 1).unwrap(), ["鸟"]);
        assert_eq!(
            h.catalog
                .query_row("SELECT count(*) FROM photo_ai_attempts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn removed_or_reused_photo_identity_is_skipped_without_inference() {
        for reused in [false, true] {
            let h = Harness::new();
            let id = h.run(false);
            if reused {
                h.catalog
                    .execute(
                        "UPDATE assets SET organization_uid='another-photo' WHERE id=1",
                        [],
                    )
                    .unwrap();
            } else {
                h.catalog
                    .execute("DELETE FROM assets WHERE id=1", [])
                    .unwrap();
            }
            let mut engine = FixedEngine {
                calls: 0,
                fail: false,
                before: None,
            };
            assert!(step(&h, &mut engine).unwrap());
            assert_eq!(engine.calls, 0);
            assert_eq!(h.task(id).skipped, 1);
            assert_eq!(
                h.catalog
                    .query_row("SELECT count(*) FROM photo_ai_results", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
    #[test]
    fn cancel_during_inference_cannot_replace_tags_and_masks_survive_success() {
        let h = Harness::new();
        photo_tags::retain_manual(&h.catalog, 1, "手动", 1).unwrap();
        photo_tags::mask(&h.catalog, 1, "鸟", true, 1).unwrap();
        let run = h.run(false);
        let mut engine = FixedEngine {
            calls: 0,
            fail: false,
            before: Some(Box::new(|| {
                h.coordinator.action(&h.app, run, "cancel").unwrap();
            })),
        };
        assert!(step(&h, &mut engine).unwrap());
        assert_eq!(h.task(run).state, "cancelled");
        assert!(
            photo_tags::snapshot(&h.catalog, 1)
                .unwrap()
                .result
                .is_none()
        );
        engine.before = None;
        let next = h.run(false);
        assert!(step(&h, &mut engine).unwrap());
        assert_eq!(h.task(next).done, 1);
        assert_eq!(
            photo_tags::effective_names(&h.catalog, 1).unwrap(),
            ["手动"]
        );
        let state = photo_tags::snapshot(&h.catalog, 1).unwrap();
        assert_eq!(state.ai, ["鸟"]);
        assert_eq!(state.masks, ["鸟"]);
    }
}
