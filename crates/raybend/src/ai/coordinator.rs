//! app/catalog 之间的短提交屏障。取消和最终提交串行；推理期间不占此锁或数据库写者。
use crate::{
    Error, Result,
    store::{db::AppDb, time},
};
use std::sync::Mutex;
#[derive(Default)]
pub struct Coordinator {
    commit: Mutex<()>,
}
impl Coordinator {
    pub fn finish(
        &self,
        app: &AppDb,
        claim: &super::jobs::Claim,
        commit: impl FnOnce() -> Result<bool>,
    ) -> Result<bool> {
        self.finish_as(app, claim, false, commit)
    }
    pub fn finish_as(
        &self,
        app: &AppDb,
        claim: &super::jobs::Claim,
        skipped: bool,
        commit: impl FnOnce() -> Result<bool>,
    ) -> Result<bool> {
        let _guard = self
            .commit
            .lock()
            .map_err(|_| Error::Unsupported("AI 提交锁损坏".into()))?;
        if !app.read(|c| super::jobs::active(c, claim))? {
            return Ok(false);
        }
        if !commit()? {
            return Ok(false);
        }
        app.write({
            let claim = claim.clone();
            move |c| {
                if skipped {
                    super::jobs::skip(c, &claim, time::now_millis())
                } else {
                    super::jobs::complete(c, &claim, time::now_millis())
                }
            }
        })
    }
    pub fn action(&self, app: &AppDb, id: i64, action: &str) -> Result<bool> {
        let _guard = self
            .commit
            .lock()
            .map_err(|_| Error::Unsupported("AI 提交锁损坏".into()))?;
        let action = action.to_owned();
        app.write(move |c| super::status::action(c, id, &action, time::now_millis()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancel_prevents_catalog_callback_but_pause_allows_the_current_photo() {
        let dir = tempfile::tempdir().unwrap();
        let app = AppDb::open(dir.path(), 1).unwrap();
        for cancelled in [false, true] {
            let id = app
                .write(|c| super::super::jobs::create(c, "测试", &"a".repeat(64), 1))
                .unwrap();
            app.write(move |c| {
                super::super::jobs::append_page(
                    c,
                    id,
                    &[crate::store::organization::PhotoRef {
                        repository_id: "库".into(),
                        asset_id: 1,
                        photo_uid: "uid".into(),
                    }],
                    1,
                )?;
                super::super::jobs::finish_prepare(c, id, 1)
            })
            .unwrap();
            let claim = app
                .write(|c| super::super::jobs::claim_next(c, 1))
                .unwrap()
                .unwrap();
            let coord = Coordinator::default();
            coord
                .action(&app, id, if cancelled { "cancel" } else { "pause" })
                .unwrap();
            let called = std::cell::Cell::new(false);
            assert_eq!(
                coord
                    .finish(&app, &claim, || {
                        called.set(true);
                        Ok(true)
                    })
                    .unwrap(),
                !cancelled
            );
            assert_eq!(called.get(), !cancelled);
        }
    }
}
