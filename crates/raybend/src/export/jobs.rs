//! Authoritative session-only export queues. Heavy work never holds state/catalog locks.
use super::{Preset, VariantSnapshot};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub repository_id: String,
    pub root: String,
    pub snapshot: VariantSnapshot,
    pub preset: Preset,
    pub status: String,
    pub error: Option<String>,
    pub sequence: u64,
    #[serde(default)]
    pub output: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub revision: u64,
    pub generation: u64,
    pub queues: BTreeMap<String, Vec<Item>>,
    pub enabled: BTreeSet<String>,
}
#[derive(Debug)]
pub enum Outcome {
    Done(String),
    FileExists(String),
    Skipped,
}
type Executor = dyn Fn(&Item) -> Result<Outcome, String> + Send + Sync;
type Notify = dyn Fn(View) + Send + Sync;
#[derive(Default)]
struct State {
    view: View,
    active: BTreeSet<(u64, String)>,
    sequence: u64,
}
#[derive(Clone)]
pub struct Engine {
    state: Arc<Mutex<State>>,
    execute: Arc<Executor>,
    notify: Arc<Notify>,
}
impl Engine {
    pub fn new(
        execute: impl Fn(&Item) -> Result<Outcome, String> + Send + Sync + 'static,
        notify: impl Fn(View) + Send + Sync + 'static,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            execute: Arc::new(execute),
            notify: Arc::new(notify),
        }
    }
    pub fn view(&self) -> View {
        self.state.lock().unwrap().view.clone()
    }
    fn change(
        &self,
        update: impl FnOnce(&mut State) -> Result<(), String>,
    ) -> Result<View, String> {
        let view = {
            let mut state = self.state.lock().map_err(|_| "导出队列锁损坏")?;
            update(&mut state)?;
            state.view.revision += 1;
            state.view.clone()
        };
        (self.notify)(view);
        self.kick();
        Ok(self.view())
    }
    pub fn enqueue(&self, generation: u64, items: Vec<Item>) -> Result<View, String> {
        if items.len() > super::MAX_BATCH {
            return Err("一次最多入队 128 个定稿".into());
        }
        self.change(|s| {
            if generation != s.view.generation {
                return Err("队列已重置，请重新入队".into());
            }
            if items
                .iter()
                .any(|i| !i.preset.validate(false).errors.is_empty())
            {
                return Err("入队预设无效".into());
            }
            for mut item in items {
                let queue = s.view.queues.entry(item.preset.id.clone()).or_default();
                if queue.iter().any(|i| {
                    i.repository_id == item.repository_id
                        && i.snapshot.reference.asset_id == item.snapshot.reference.asset_id
                        && i.snapshot.profile_hash == item.snapshot.profile_hash
                }) {
                    continue;
                }
                s.sequence += 1;
                item.sequence = s.sequence;
                item.id = format!("{}:{}:{}", s.view.generation, item.preset.id, s.sequence);
                item.status = "pending".into();
                item.error = None;
                item.output = None;
                queue.insert(0, item);
            }
            Ok(())
        })
    }
    pub fn enable(&self, id: String, on: bool) -> Result<View, String> {
        self.change(|s| {
            if on {
                if !s.view.enabled.contains(&id) && s.view.enabled.len() >= 4 {
                    return Err("同时最多开启四个导出预设，请先关闭其他预设".into());
                }
                s.view.enabled.insert(id);
            } else {
                s.view.enabled.remove(&id);
            }
            Ok(())
        })
    }
    pub fn stop_all(&self) -> Result<View, String> {
        self.change(|s| {
            s.view.enabled.clear();
            Ok(())
        })
    }
    pub fn reset(&self) -> Result<View, String> {
        self.change(|s| {
            s.view.generation += 1;
            s.view.enabled.clear();
            s.view.queues.clear();
            Ok(())
        })
    }
    pub fn remove(&self, ids: &BTreeSet<String>) -> Result<View, String> {
        self.change(|s| {
            for q in s.view.queues.values_mut() {
                q.retain(|i| {
                    !ids.contains(&i.id) || !matches!(i.status.as_str(), "pending" | "failed")
                });
            }
            Ok(())
        })
    }
    pub fn retry(&self, ids: &BTreeSet<String>) -> Result<View, String> {
        self.change(|s| {
            for q in s.view.queues.values_mut() {
                for i in q {
                    if ids.contains(&i.id) && i.status == "failed" && i.output.is_none() {
                        i.status = "pending".into();
                        i.error = None;
                    }
                }
            }
            Ok(())
        })
    }
    fn kick(&self) {
        let mut launches = Vec::new();
        let view = {
            let mut s = self.state.lock().unwrap();
            let epoch = s.view.generation;
            for id in s.view.enabled.clone() {
                if s.active.len() >= 4 {
                    break;
                }
                if s.active.iter().any(|(_, p)| p == &id) {
                    continue;
                }
                let Some(q) = s.view.queues.get_mut(&id) else {
                    continue;
                };
                let Some(item) = q.iter_mut().rev().find(|i| i.status == "pending") else {
                    continue;
                };
                item.status = "running".into();
                launches.push(item.clone());
                s.active.insert((epoch, id));
            }
            if launches.is_empty() {
                return;
            }
            s.view.revision += 1;
            s.view.clone()
        };
        (self.notify)(view.clone());
        for item in launches {
            let engine = self.clone();
            let epoch = view.generation;
            let preset = item.preset.id.clone();
            let fallback = self.clone();
            let fallback_preset = preset.clone();
            let spawn = std::thread::Builder::new()
                .name(format!("export-{preset}"))
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (engine.execute)(&item)
                    }))
                    .unwrap_or_else(|_| Err("导出线程异常".into()));
                    let view = {
                        let mut s = engine.state.lock().unwrap();
                        s.active.remove(&(epoch, preset));
                        if s.view.generation == epoch {
                            if matches!(&result, Ok(Outcome::Skipped)) {
                                // Invalid hashes leave no history, selection or dedup shadow.
                                for queue in s.view.queues.values_mut() {
                                    queue.retain(|saved| saved.id != item.id);
                                }
                            } else if let Some(saved) = s
                                .view
                                .queues
                                .values_mut()
                                .flat_map(|queue| queue.iter_mut())
                                .find(|saved| saved.id == item.id)
                            {
                                match result {
                                    Ok(Outcome::Done(path)) => {
                                        saved.status = "done".into();
                                        saved.output = Some(path);
                                        saved.error = None;
                                    }
                                    Ok(Outcome::FileExists(path)) => {
                                        saved.status = "skipped".into();
                                        saved.output = Some(path);
                                        saved.error = None;
                                    }
                                    Err(error) => {
                                        saved.status = "failed".into();
                                        saved.error = Some(error);
                                    }
                                    Ok(Outcome::Skipped) => unreachable!(),
                                }
                            }
                        }
                        s.view.revision += 1;
                        s.view.clone()
                    };
                    (engine.notify)(view);
                    engine.kick();
                });
            if let Err(e) = spawn {
                let view = {
                    let mut s = fallback.state.lock().unwrap();
                    s.active.remove(&(epoch, fallback_preset.clone()));
                    if let Some(item) = s
                        .view
                        .queues
                        .get_mut(&fallback_preset)
                        .and_then(|q| q.iter_mut().find(|i| i.status == "running"))
                    {
                        item.status = "failed".into();
                        item.error = Some(e.to_string());
                    }
                    s.view.revision += 1;
                    s.view.clone()
                };
                (fallback.notify)(view);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::{Duration, Instant};
    fn item(p: &str, n: i64) -> Item {
        let stack = crate::store::develop::DevelopStack::default();
        Item {
            id: String::new(),
            repository_id: "repo".into(),
            root: "/root".into(),
            snapshot: VariantSnapshot {
                reference: super::super::VariantRef {
                    asset_id: n,
                    variant: "raw".into(),
                },
                name: "RAW".into(),
                rel_path: "test.raw".into(),
                profile_hash: crate::store::issues::profile_hash(&stack).unwrap(),
                stack,
                source_signature: "sig".into(),
            },
            preset: Preset {
                id: p.into(),
                name: p.into(),
                format: "png".into(),
                quality: 90,
                max_edge: 0,
                size_mode: crate::export::SizeMode::Original,
                percent: 100,
                directory: "/tmp".into(),
                template: ":FILENAME".into(),
                existing_file: crate::export::ExistingFile::Append,
            },
            status: "pending".into(),
            error: None,
            sequence: 0,
            output: None,
        }
    }
    fn wait(engine: &Engine, p: impl Fn(&View) -> bool) {
        let start = Instant::now();
        while !p(&engine.view()) {
            assert!(start.elapsed() < Duration::from_secs(3));
            std::thread::yield_now();
        }
    }
    #[test]
    fn existing_file_skip_is_terminal_deduplicated_and_not_hash_invalidation() {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let engine = Engine::new(
            move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(Outcome::FileExists("existing.png".into()))
            },
            |_| {},
        );
        engine.enqueue(0, vec![item("p", 1)]).unwrap();
        engine.enable("p".into(), true).unwrap();
        wait(&engine, |v| v.queues["p"][0].status == "skipped");
        engine.enqueue(0, vec![item("p", 1)]).unwrap();
        let view = engine.view();
        assert_eq!(view.queues["p"].len(), 1);
        assert_eq!(view.queues["p"][0].output.as_deref(), Some("existing.png"));
        assert!(view.queues["p"][0].error.is_none());
        engine
            .retry(&BTreeSet::from([view.queues["p"][0].id.clone()]))
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(engine.view().queues["p"][0].status, "skipped");
    }

    #[test]
    fn fifo_dedup_fail_continue_and_retry() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let copy = seen.clone();
        let engine = Engine::new(
            move |i| {
                copy.lock().unwrap().push(i.snapshot.reference.asset_id);
                if i.snapshot.reference.asset_id == 1 {
                    Err("bad".into())
                } else {
                    Ok(Outcome::Done("out".into()))
                }
            },
            |_| {},
        );
        engine
            .enqueue(0, vec![item("p", 1), item("p", 2), item("p", 2)])
            .unwrap();
        engine.enable("p".into(), true).unwrap();
        wait(&engine, |v| {
            v.queues["p"]
                .iter()
                .all(|i| i.status != "pending" && i.status != "running")
        });
        assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
        let view = engine.view();
        let done = view.queues["p"]
            .iter()
            .find(|i| i.status == "done")
            .unwrap()
            .id
            .clone();
        engine.retry(&BTreeSet::from([done])).unwrap();
        assert_eq!(seen.lock().unwrap().len(), 2);
    }
    #[test]
    fn four_workers_reset_holds_real_budget() {
        let active = Arc::new(AtomicUsize::new(0));
        let max = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::channel();
        let gate = Arc::new(Mutex::new(rx));
        let (a, m, g) = (active.clone(), max.clone(), gate.clone());
        let engine = Engine::new(
            move |_| {
                let n = a.fetch_add(1, Ordering::SeqCst) + 1;
                m.fetch_max(n, Ordering::SeqCst);
                g.lock().unwrap().recv().unwrap();
                a.fetch_sub(1, Ordering::SeqCst);
                Ok(Outcome::Done("x".into()))
            },
            |_| {},
        );
        for i in 0..4 {
            let id = format!("p{i}");
            engine.enqueue(0, vec![item(&id, i + 1)]).unwrap();
            engine.enable(id, true).unwrap();
        }
        wait(&engine, |_| active.load(Ordering::SeqCst) == 4);
        assert!(engine.enable("fifth".into(), true).is_err());
        engine.reset().unwrap();
        assert!(engine.view().queues.is_empty());
        engine.enqueue(1, vec![item("new", 9)]).unwrap();
        engine.enable("new".into(), true).unwrap();
        assert_eq!(engine.view().queues["new"][0].status, "pending");
        for _ in 0..5 {
            tx.send(()).unwrap()
        }
        wait(&engine, |v| v.queues["new"][0].status == "done");
        assert!(max.load(Ordering::SeqCst) <= 4);
        assert_eq!(engine.view().queues.len(), 1);
        assert!(engine.enqueue(0, vec![item("stale", 99)]).is_err());
    }
    #[test]
    fn invalid_hash_is_removed_and_next_item_runs_without_a_dedup_shadow() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let copy = seen.clone();
        let engine = Engine::new(
            move |i| {
                copy.lock().unwrap().push(i.snapshot.reference.asset_id);
                if i.snapshot.reference.asset_id == 1 {
                    Ok(Outcome::Skipped)
                } else {
                    Ok(Outcome::Done("out".into()))
                }
            },
            |_| {},
        );
        engine.enqueue(0, vec![item("p", 1), item("p", 2)]).unwrap();
        engine.enable("p".into(), true).unwrap();
        wait(&engine, |v| {
            v.queues["p"].len() == 1 && v.queues["p"][0].status == "done"
        });
        assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
        engine.enable("p".into(), false).unwrap();
        engine.enqueue(0, vec![item("p", 1)]).unwrap();
        assert_eq!(
            engine.view().queues["p"].len(),
            2,
            "removed hash can be enqueued again"
        );
        assert_eq!(engine.view().queues["p"][0].status, "pending");
        engine.enable("p".into(), true).unwrap();
        wait(&engine, |v| v.queues["p"].len() == 1);
        assert_eq!(*seen.lock().unwrap(), vec![1, 2, 1]);
    }
    #[test]
    fn invalid_last_item_leaves_empty_enabled_queue_without_a_worker() {
        let engine = Engine::new(|_| Ok(Outcome::Skipped), |_| {});
        engine.enqueue(0, vec![item("p", 1)]).unwrap();
        engine.enable("p".into(), true).unwrap();
        wait(&engine, |v| v.queues["p"].is_empty());
        assert!(engine.view().enabled.contains("p"));
        assert!(engine.state.lock().unwrap().active.is_empty());
    }
    #[test]
    fn empty_on_and_stop_keep_state() {
        let engine = Engine::new(|_| panic!("empty must not run"), |_| {});
        engine.enable("p".into(), true).unwrap();
        assert!(engine.view().queues.is_empty());
        engine.stop_all().unwrap();
        assert!(engine.view().enabled.is_empty());
    }
}
