//! 与操作系统显示色彩状态的接缝。核心只消费事实，不在这里调用 Win32/Tauri。
//! Windows 实现、无头 fake 与将来的 macOS/Linux 实现均遵循同一接口。

use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 物理桌面坐标，调用方先完成 WebView CSS/DPR 到物理像素的换算。
/// 与编辑视口契约相同，不能在 adapter 内猜 DPR。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalWindowRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl PhysicalWindowRect {
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DisplayColorState {
    /// 传统 SDR：由应用把照片转换到此显示器 ICC；是否已校准不能仅凭路径判定。
    Icc { profile_path: PathBuf },
    /// Windows Advanced Color/ACM：应用交付声明好的标准空间，由系统映射。
    SystemManaged {
        output_space: SystemOutputSpace,
        sdr_white_nits: Option<f32>,
    },
    /// 没有可靠 ICC 时的显式回退；不能显示为“已校准”。
    SrgbFallback { reason: String },
    /// 探测不可用或尚未实现。消费者必须保留这一状态，不可猜为 ICC 或 ACM。
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SystemOutputSpace {
    Srgb,
    ScRgb,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplaySnapshot {
    /// 仅本机会话内有效的显示目标身份，不进入照片/issue/XMP。
    pub display_id: Option<String>,
    pub state: DisplayColorState,
}

impl DisplaySnapshot {
    fn validate(&self) -> Result<(), ColorSystemError> {
        if self
            .display_id
            .as_deref()
            .is_some_and(|id| id.is_empty() || id.len() > 512 || id.contains('\0'))
        {
            return Err(ColorSystemError::InvalidSnapshot("display id"));
        }
        match &self.state {
            DisplayColorState::Icc { profile_path } if profile_path.as_os_str().is_empty() => {
                Err(ColorSystemError::InvalidSnapshot("empty ICC path"))
            }
            DisplayColorState::SystemManaged {
                sdr_white_nits: Some(nits),
                ..
            } if !nits.is_finite() || *nits <= 0.0 || *nits > 10_000.0 => {
                Err(ColorSystemError::InvalidSnapshot("invalid SDR white level"))
            }
            DisplayColorState::SrgbFallback { reason }
            | DisplayColorState::Unavailable { reason }
                if reason.trim().is_empty() =>
            {
                Err(ColorSystemError::InvalidSnapshot("empty state reason"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ColorSystemError {
    #[error("invalid window identity")]
    InvalidWindowId,
    #[error("invalid physical window rectangle")]
    InvalidWindowRect,
    #[error("invalid system color snapshot: {0}")]
    InvalidSnapshot(&'static str),
    #[error("system color settings are unavailable: {0}")]
    Unavailable(String),
}

/// 平台接口的读取语义：每次调用都重新读取当前系统事实；节流由事件调用方决定。
/// `open_color_settings` 只打开系统设置页面，不修改系统 ICC/HDR/校准状态。
pub trait ColorSystemAdapter: Send + Sync {
    fn snapshot(&self, window: PhysicalWindowRect) -> Result<DisplaySnapshot, ColorSystemError>;
    fn open_color_settings(&self) -> Result<(), ColorSystemError>;
    fn read_display_profile(&self, _path: &std::path::Path) -> Result<super::icc::RgbIcc, ColorSystemError> {
        Err(ColorSystemError::Unavailable("display ICC reader unavailable".into()))
    }
    /// 订阅系统色彩状态的失效提示。回调只表示“需要重读”，不能携带半更新快照。
    /// 返回值的 Drop 必须释放平台监听；不支持事件的平台可保留默认错误，调用方
    /// 仍可在恢复焦点或用户手动刷新时重新读取。
    fn subscribe_changes(
        &self,
        _invalidate: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Box<dyn ColorSystemSubscription>, ColorSystemError> {
        Err(ColorSystemError::Unavailable(
            "change subscription unavailable".into(),
        ))
    }
}

/// 平台监听句柄；释放对象即取消监听。具体平台句柄不越过 adapter 边界。
pub trait ColorSystemSubscription: Send {}
impl<T: Send> ColorSystemSubscription for T {}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionedDisplaySnapshot {
    /// 本机会话内的刷新代数，绝不进入照片、issue、XMP 或成片。
    pub generation: u64,
    pub snapshot: DisplaySnapshot,
}

#[derive(Default)]
struct WindowColorState {
    latest_request: u64,
    accepted: Option<VersionedDisplaySnapshot>,
}

/// 多窗口共用的显示色彩状态。每次刷新都重新问 adapter；并发刷新只有最后一次能落地。
/// App/编辑器/全屏窗口只消费此服务的结果，不分别维护一套屏幕选择规则。
pub struct ColorEnvironment {
    adapter: Arc<dyn ColorSystemAdapter>,
    next_generation: AtomicU64,
    windows: Mutex<HashMap<String, WindowColorState>>,
}

impl ColorEnvironment {
    #[must_use]
    pub fn new(adapter: Arc<dyn ColorSystemAdapter>) -> Self {
        Self {
            adapter,
            next_generation: AtomicU64::new(1),
            windows: Mutex::new(HashMap::new()),
        }
    }

    pub fn refresh(
        &self,
        window_id: &str,
        rect: PhysicalWindowRect,
    ) -> Result<Option<VersionedDisplaySnapshot>, ColorSystemError> {
        if window_id.is_empty() || window_id.len() > 512 {
            return Err(ColorSystemError::InvalidWindowId);
        }
        if !rect.is_valid() {
            return Err(ColorSystemError::InvalidWindowRect);
        }
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        {
            let mut windows = self
                .windows
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let state = windows.entry(window_id.to_string()).or_default();
            state.latest_request = generation;
            state.accepted = None;
        }
        let snapshot = read_display(self.adapter.as_ref(), rect)?;
        let mut windows = self
            .windows
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(state) = windows.get_mut(window_id) else {
            return Ok(None);
        };
        if state.latest_request != generation {
            return Ok(None);
        }
        let accepted = VersionedDisplaySnapshot {
            generation,
            snapshot,
        };
        state.accepted = Some(accepted.clone());
        Ok(Some(accepted))
    }

    /// 跨屏、profile/ACM 变化、焦点恢复与手动刷新均走同一失效入口。
    /// 失效后不能继续把旧显示配置当成当前配置。
    pub fn invalidate(&self, window_id: &str) {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let mut windows = self
            .windows
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(state) = windows.get_mut(window_id) {
            state.latest_request = generation;
            state.accepted = None;
        }
    }

    #[must_use]
    pub fn current(&self, window_id: &str) -> Option<VersionedDisplaySnapshot> {
        self.windows
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(window_id)
            .and_then(|state| state.accepted.clone())
    }

    /// 窗口销毁时丢掉状态；未完成的刷新不能让它复活。
    pub fn forget(&self, window_id: &str) {
        self.windows
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(window_id);
    }

    pub fn open_color_settings(&self) -> Result<(), ColorSystemError> {
        self.adapter.open_color_settings()
    }

    pub fn subscribe_changes(&self, invalidate: Arc<dyn Fn() + Send + Sync>) -> Result<Box<dyn ColorSystemSubscription>, ColorSystemError> {
        self.adapter.subscribe_changes(invalidate)
    }

    pub fn invalidate_all(&self) {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        for state in self.windows.lock().unwrap_or_else(|e| e.into_inner()).values_mut() {
            state.latest_request = generation;
            state.accepted = None;
        }
    }

    /// Prepare off the render thread, then discard if newer facts have arrived.
    pub fn prepare_current(&self, window_id: &str) -> Result<Option<super::display::PreparedDisplay>, ColorSystemError> {
        let Some(current) = self.current(window_id) else { return Ok(None); };
        let prepared = super::display::prepare_display(self.adapter.as_ref(), &current)?;
        Ok(self.current(window_id).filter(|state| state.generation == current.generation).map(|_| prepared))
    }
}

/// 所有消费者统一经过此验证入口，不能直接把损坏的系统事实送进 GPU 转换。
pub fn read_display(
    adapter: &dyn ColorSystemAdapter,
    window: PhysicalWindowRect,
) -> Result<DisplaySnapshot, ColorSystemError> {
    if !window.is_valid() {
        return Err(ColorSystemError::InvalidWindowRect);
    }
    let snapshot = adapter.snapshot(window)?;
    snapshot.validate()?;
    Ok(snapshot)
}

/// 浏览器、无头测试和未接平台实现的明确降级。
pub struct UnsupportedColorSystemAdapter;

/// Coalesces display invalidations on a dedicated worker. Callbacks only receive
/// prepared data; neither a window handle nor disk/ICC work reaches rendering.
#[derive(Clone)]
pub struct DisplayWatcher {
    pending: Arc<Mutex<DisplayRequest>>,
    wake: std::sync::mpsc::SyncSender<()>,
}

struct DisplayRequest {
    revision: u64,
    rect: PhysicalWindowRect,
    force: bool,
    closed: bool,
}

impl DisplayWatcher {
    pub fn new(
        environment: Arc<ColorEnvironment>, window_id: String, rect: PhysicalWindowRect,
        publish: impl Fn(Result<super::display::PreparedDisplay, ColorSystemError>) + Send + 'static,
    ) -> Result<Self, ColorSystemError> {
        if !rect.is_valid() { return Err(ColorSystemError::InvalidWindowRect); }
        if window_id.is_empty() || window_id.len() > 512 { return Err(ColorSystemError::InvalidWindowId); }
        let pending = Arc::new(Mutex::new(DisplayRequest { revision: 0, rect, force: true, closed: false }));
        let (wake, receiver) = std::sync::mpsc::sync_channel(1);
        let work = Arc::clone(&pending);
        std::thread::Builder::new().name("color-display".into()).spawn(move || {
            let mut cached: Option<(DisplaySnapshot, super::display::DisplayStrategy)> = None;
            while receiver.recv().is_ok() {
                while receiver.try_recv().is_ok() {}
                let (revision, rect, force) = {
                    let mut request = work.lock().unwrap_or_else(|e| e.into_inner());
                    if request.closed { break; }
                    let facts = (request.revision, request.rect, request.force);
                    request.force = false;
                    facts
                };
                let result = environment.refresh(&window_id, rect).and_then(|snapshot| {
                    let Some(snapshot) = snapshot else { return Ok(None); };
                    if !force && let Some((facts, strategy)) = &cached && facts == &snapshot.snapshot {
                        return Ok(Some(super::display::PreparedDisplay { generation: snapshot.generation, snapshot: snapshot.snapshot.clone(), strategy: strategy.clone() }));
                    }
                    let prepared = environment.prepare_current(&window_id)?;
                    if let Some(prepared) = &prepared { cached = Some((snapshot.snapshot, prepared.strategy.clone())); }
                    Ok(prepared)
                });
                if result.is_err() { cached = None; }
                // Sequence check and delivery share this lock. A newer request
                // cannot overtake a checked result immediately before publish.
                let request = work.lock().unwrap_or_else(|e| e.into_inner());
                if request.closed { break; }
                if request.revision == revision {
                    match result { Ok(Some(prepared)) => publish(Ok(prepared)), Err(error) => publish(Err(error)), Ok(None) => {} }
                }
            }
            environment.forget(&window_id);
        }).map_err(|error| ColorSystemError::Unavailable(error.to_string()))?;
        let watcher = Self { pending, wake };
        watcher.refresh(true);
        Ok(watcher)
    }

    /// Window events provide physical facts directly; no window query is made.
    pub fn update(&self, change: impl FnOnce(&mut PhysicalWindowRect), force: bool) {
        let mut request = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if request.closed { return; }
        change(&mut request.rect);
        if !request.rect.is_valid() { return; }
        request.revision = request.revision.wrapping_add(1);
        request.force |= force;
        let _ = self.wake.try_send(());
    }

    pub fn refresh(&self, force: bool) { self.update(|_| {}, force); }

    pub fn close(&self) {
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        let _ = self.wake.try_send(());
    }

    pub fn is_closed(&self) -> bool { self.pending.lock().unwrap_or_else(|e| e.into_inner()).closed }
}

impl ColorSystemAdapter for UnsupportedColorSystemAdapter {
    fn snapshot(&self, window: PhysicalWindowRect) -> Result<DisplaySnapshot, ColorSystemError> {
        if !window.is_valid() {
            return Err(ColorSystemError::InvalidWindowRect);
        }
        Ok(DisplaySnapshot {
            display_id: None,
            state: DisplayColorState::Unavailable {
                reason: "platform adapter not connected".into(),
            },
        })
    }

    fn open_color_settings(&self) -> Result<(), ColorSystemError> {
        Err(ColorSystemError::Unavailable(
            "platform adapter not connected".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    struct FakeAdapter {
        snapshots: Mutex<Vec<DisplaySnapshot>>,
    }

    impl ColorSystemAdapter for FakeAdapter {
        fn snapshot(
            &self,
            window: PhysicalWindowRect,
        ) -> Result<DisplaySnapshot, ColorSystemError> {
            if !window.is_valid() {
                return Err(ColorSystemError::InvalidWindowRect);
            }
            Ok(self.snapshots.lock().unwrap().remove(0))
        }

        fn open_color_settings(&self) -> Result<(), ColorSystemError> {
            Ok(())
        }
    }

    #[test]
    fn fake_adapter_can_report_monitor_change_without_photo_state() {
        let adapter: Arc<dyn ColorSystemAdapter> = Arc::new(FakeAdapter {
            snapshots: Mutex::new(vec![
                DisplaySnapshot {
                    display_id: Some("screen-a".into()),
                    state: DisplayColorState::Icc {
                        profile_path: PathBuf::from("C:/显示器/一号.icc"),
                    },
                },
                DisplaySnapshot {
                    display_id: Some("screen-b".into()),
                    state: DisplayColorState::SystemManaged {
                        output_space: SystemOutputSpace::Srgb,
                        sdr_white_nits: Some(203.0),
                    },
                },
            ]),
        });
        let rect = PhysicalWindowRect {
            x: -1920,
            y: 0,
            width: 1200,
            height: 800,
        };
        let first = read_display(adapter.as_ref(), rect).unwrap();
        let second = read_display(adapter.as_ref(), PhysicalWindowRect { x: 0, ..rect }).unwrap();
        assert_eq!(first.display_id.as_deref(), Some("screen-a"));
        assert_eq!(second.display_id.as_deref(), Some("screen-b"));
        assert!(matches!(
            second.state,
            DisplayColorState::SystemManaged { .. }
        ));
        adapter.open_color_settings().unwrap();
    }

    #[test]
    fn unsupported_is_explicit_and_rejects_zero_size() {
        let adapter = UnsupportedColorSystemAdapter;
        let rect = PhysicalWindowRect {
            x: i32::MAX,
            y: i32::MIN,
            width: 1,
            height: 1,
        };
        assert!(matches!(
            adapter.snapshot(rect).unwrap().state,
            DisplayColorState::Unavailable { .. }
        ));
        assert!(matches!(
            adapter.snapshot(PhysicalWindowRect { width: 0, ..rect }),
            Err(ColorSystemError::InvalidWindowRect)
        ));
        assert!(adapter.open_color_settings().is_err());
    }

    #[test]
    fn invalid_adapter_snapshot_never_reaches_consumer() {
        let rect = PhysicalWindowRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let adapter = FakeAdapter {
            snapshots: Mutex::new(vec![DisplaySnapshot {
                display_id: Some("screen".into()),
                state: DisplayColorState::SystemManaged {
                    output_space: SystemOutputSpace::ScRgb,
                    sdr_white_nits: Some(f32::NAN),
                },
            }]),
        };
        assert!(matches!(
            read_display(&adapter, rect),
            Err(ColorSystemError::InvalidSnapshot(_))
        ));
        assert!(matches!(
            read_display(
                &UnsupportedColorSystemAdapter,
                PhysicalWindowRect { height: 0, ..rect }
            ),
            Err(ColorSystemError::InvalidWindowRect)
        ));
    }

    #[test]
    fn environment_reloads_after_invalidation_and_forgets_closed_window() {
        let adapter = Arc::new(FakeAdapter {
            snapshots: Mutex::new(vec![
                DisplaySnapshot {
                    display_id: Some("A".into()),
                    state: DisplayColorState::Icc {
                        profile_path: PathBuf::from("A.icc"),
                    },
                },
                DisplaySnapshot {
                    display_id: Some("B".into()),
                    state: DisplayColorState::SystemManaged {
                        output_space: SystemOutputSpace::Srgb,
                        sdr_white_nits: Some(203.0),
                    },
                },
            ]),
        });
        let environment = ColorEnvironment::new(adapter);
        let rect = PhysicalWindowRect {
            x: -1200,
            y: 0,
            width: 800,
            height: 600,
        };
        let first = environment.refresh("main", rect).unwrap().unwrap();
        assert_eq!(first.snapshot.display_id.as_deref(), Some("A"));
        environment.invalidate("main");
        assert!(environment.current("main").is_none());
        let second = environment
            .refresh("main", PhysicalWindowRect { x: 0, ..rect })
            .unwrap()
            .unwrap();
        assert!(second.generation > first.generation);
        assert_eq!(second.snapshot.display_id.as_deref(), Some("B"));
        environment.open_color_settings().unwrap();
        environment.forget("main");
        assert!(environment.current("main").is_none());
    }

    struct RacingAdapter {
        entered: Barrier,
        release: Barrier,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ColorSystemAdapter for RacingAdapter {
        fn snapshot(&self, _: PhysicalWindowRect) -> Result<DisplaySnapshot, ColorSystemError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                self.entered.wait();
                self.release.wait();
            }
            Ok(DisplaySnapshot {
                display_id: Some(format!("display-{call}")),
                state: DisplayColorState::SrgbFallback {
                    reason: "test".into(),
                },
            })
        }

        fn open_color_settings(&self) -> Result<(), ColorSystemError> {
            Ok(())
        }
    }

    #[test]
    fn delayed_monitor_snapshot_cannot_replace_newer_one() {
        let adapter = Arc::new(RacingAdapter {
            entered: Barrier::new(2),
            release: Barrier::new(2),
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let environment = Arc::new(ColorEnvironment::new(adapter.clone()));
        let rect = PhysicalWindowRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        std::thread::scope(|scope| {
            let old = scope.spawn(|| environment.refresh("main", rect).unwrap());
            adapter.entered.wait();
            let latest = environment.refresh("main", rect).unwrap().unwrap();
            assert_eq!(latest.snapshot.display_id.as_deref(), Some("display-1"));
            adapter.release.wait();
            assert!(old.join().unwrap().is_none());
            assert_eq!(environment.current("main"), Some(latest));
        });
    }

    struct PreparedAdapter {
        reads: std::sync::atomic::AtomicUsize,
        managed: bool,
        block: Option<(Barrier, Barrier)>,
    }
    impl ColorSystemAdapter for PreparedAdapter {
        fn snapshot(&self, _: PhysicalWindowRect) -> Result<DisplaySnapshot, ColorSystemError> {
            Ok(DisplaySnapshot { display_id: Some("显示器".into()), state: if self.managed {
                DisplayColorState::SystemManaged { output_space: SystemOutputSpace::Srgb, sdr_white_nits: Some(203.0) }
            } else { DisplayColorState::Icc { profile_path: "显示器.icc".into() } } })
        }
        fn open_color_settings(&self) -> Result<(), ColorSystemError> { Ok(()) }
        fn read_display_profile(&self, _: &std::path::Path) -> Result<super::super::icc::RgbIcc, ColorSystemError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if let Some((entered, release)) = &self.block { entered.wait(); release.wait(); }
            Ok(super::super::icc::srgb_icc().unwrap())
        }
    }

    fn rect() -> PhysicalWindowRect { PhysicalWindowRect { x: 0, y: 0, width: 100, height: 100 } }

    #[test]
    fn system_managed_preparation_never_reads_or_applies_display_icc() {
        let adapter = Arc::new(PreparedAdapter { reads: 0.into(), managed: true, block: None });
        let environment = ColorEnvironment::new(adapter.clone());
        environment.refresh("main", rect()).unwrap();
        assert!(matches!(environment.prepare_current("main").unwrap().unwrap().strategy, super::super::display::DisplayStrategy::SystemManaged { .. }));
        assert_eq!(adapter.reads.load(Ordering::SeqCst), 0);
        environment.invalidate_all();
        assert!(environment.prepare_current("main").unwrap().is_none());
    }

    #[test]
    fn invalidation_during_icc_preparation_discards_the_old_transform() {
        let adapter = Arc::new(PreparedAdapter { reads: 0.into(), managed: false, block: Some((Barrier::new(2), Barrier::new(2))) });
        let environment = ColorEnvironment::new(adapter.clone());
        environment.refresh("main", rect()).unwrap();
        std::thread::scope(|scope| {
            let prepared = scope.spawn(|| environment.prepare_current("main").unwrap());
            adapter.block.as_ref().unwrap().0.wait();
            environment.invalidate_all();
            adapter.block.as_ref().unwrap().1.wait();
            assert!(prepared.join().unwrap().is_none());
        });
    }

    #[test]
    fn display_watcher_reuses_preparation_on_same_screen_and_reloads_when_forced() {
        let adapter = Arc::new(PreparedAdapter { reads: 0.into(), managed: false, block: None });
        let environment = Arc::new(ColorEnvironment::new(adapter.clone()));
        let (send, receive) = std::sync::mpsc::channel();
        let watcher = DisplayWatcher::new(environment, "main".into(), rect(), move |result| { let _ = send.send(result); }).unwrap();
        let first = receive.recv_timeout(std::time::Duration::from_secs(2)).unwrap().unwrap();
        watcher.update(|rect| rect.x = -1920, false);
        let moved = receive.recv_timeout(std::time::Duration::from_secs(2)).unwrap().unwrap();
        let (super::super::display::DisplayStrategy::Icc(a), super::super::display::DisplayStrategy::Icc(b)) = (first.strategy, moved.strategy) else { panic!("expected ICC"); };
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(adapter.reads.load(Ordering::SeqCst), 1);
        watcher.refresh(true);
        receive.recv_timeout(std::time::Duration::from_secs(2)).unwrap().unwrap();
        assert_eq!(adapter.reads.load(Ordering::SeqCst), 2);
        watcher.close();
        watcher.refresh(true);
        assert!(watcher.is_closed());
        assert!(receive.recv_timeout(std::time::Duration::from_secs(2)).is_err());
    }

    #[test]
    fn display_watcher_coalesces_pending_events_and_cannot_publish_old_facts() {
        let adapter = Arc::new(RacingAdapter { entered: Barrier::new(2), release: Barrier::new(2), calls: 0.into() });
        let environment = Arc::new(ColorEnvironment::new(adapter.clone()));
        let (send, receive) = std::sync::mpsc::channel();
        let watcher = DisplayWatcher::new(environment, "main".into(), rect(), move |result| { let _ = send.send(result); }).unwrap();
        adapter.entered.wait();
        for i in 0..1000 { watcher.update(|rect| rect.x = i, false); }
        adapter.release.wait();
        let prepared = receive.recv_timeout(std::time::Duration::from_secs(2)).unwrap().unwrap();
        assert!(matches!(prepared.strategy, super::super::display::DisplayStrategy::SrgbFallback { .. }));
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 2);
        watcher.close();
        assert!(receive.recv_timeout(std::time::Duration::from_secs(2)).is_err());
    }
}
