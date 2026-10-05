//! 高质量降噪的单工作线程、最新请求优先和单结果缓存；不依赖 Tauri。
use super::{
    bm3d::denoise_high,
    denoise::DenoisePlan,
    lens::{LensCorrection, LensMap, warp_lens},
    pipeline::LinearImage,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Sender},
};

#[derive(Clone, Debug, PartialEq)]
pub struct DenoiseKey {
    /// 解码源的代号；同路径重新读盘也必须更新。
    pub source: u64,
    pub width: u32,
    pub height: u32,
    pub plan: DenoisePlan,
    pub lens: LensCorrection,
}
pub trait DenoiseImage: Sized + Send + Sync + 'static {
    fn process(&self,key:&DenoiseKey,cancel:&AtomicBool)->Option<Self>;
}
impl DenoiseImage for LinearImage {
    fn process(&self,key:&DenoiseKey,cancel:&AtomicBool)->Option<Self> {
        let warped=warp_lens(self,&LensMap::new(&key.lens));
        denoise_high(&warped,&key.plan,cancel)
    }
}
impl DenoiseImage for super::working::WorkingReferenceImage {
    fn process(&self,key:&DenoiseKey,cancel:&AtomicBool)->Option<Self> {
        let warped=super::lens::warp_rgb(self.view(),&LensMap::new(&key.lens));
        if cancel.load(Ordering::Relaxed) {return None;}
        let rgb=super::bm3d::denoise_high_rgb(super::sample::RgbView {width:self.width,height:self.height,rgb:&warped},&key.plan,cancel)?;
        Some(Self {width:self.width,height:self.height,rgb})
    }
}
type CachedResult<T> = Result<Arc<T>, String>;
struct State<T> {
    wanted: Option<DenoiseKey>,
    ready: Option<(DenoiseKey, CachedResult<T>)>,
}
impl<T> Default for State<T> {fn default()->Self {Self {wanted:None,ready:None}}}
struct Request<T> {key:DenoiseKey,image:Arc<T>,cancel:Arc<AtomicBool>}
pub struct BackgroundDenoise<T: DenoiseImage> {
    shared:Arc<Mutex<State<T>>>, sender:Option<Sender<Request<T>>>,
    thread:Option<std::thread::JoinHandle<()>>,cancel:Arc<AtomicBool>,
}
pub type HighDenoise=BackgroundDenoise<LinearImage>;
pub type WorkingHighDenoise=BackgroundDenoise<super::working::WorkingReferenceImage>;
impl<T: DenoiseImage> BackgroundDenoise<T> {
    /// 回调只通知「有结果了」，读缓存仍校验完整键，不把旧照片结果推给当前帧。
    pub fn new(notify: impl Fn() + Send + 'static) -> Self {
        let shared = Arc::new(Mutex::new(State::default()));
        let state = Arc::clone(&shared);
        let (sender, receiver) = mpsc::channel::<Request<T>>();
        let thread = std::thread::Builder::new()
            .name("develop-bm3d".into())
            .spawn(move || {
                while let Ok(mut request) = receiver.recv() {
                    while let Ok(newer) = receiver.try_recv() {
                        request = newer;
                    }
                    if request.cancel.load(Ordering::Relaxed) {
                        continue;
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        request.image.process(&request.key,&request.cancel).map(Arc::new)
                    }));
                    if request.cancel.load(Ordering::Relaxed) {
                        continue;
                    }
                    let result = match result {
                        Ok(Some(image)) => Ok(image),
                        Ok(None) => continue,
                        Err(_) => Err("BM3D worker panicked".into()),
                    };
                    let mut state = state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if state.wanted.as_ref() != Some(&request.key) {
                        continue;
                    }
                    state.ready = Some((request.key, result));
                    drop(state);
                    notify();
                }
            })
            .expect("start BM3D worker");
        Self {
            shared,
            sender: Some(sender),
            thread: Some(thread),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn get_or_request(
        &mut self,
        key: DenoiseKey,
        image: &Arc<T>,
        start: bool,
    ) -> Option<CachedResult<T>> {
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((cached_key, result)) = &state.ready
            && *cached_key == key
        {
            let result = result.clone();
            if state.wanted.as_ref() != Some(&key) {
                self.cancel.store(true, Ordering::Relaxed);
                state.wanted = Some(key);
            }
            return Some(result);
        }
        if !start {
            self.cancel.store(true, Ordering::Relaxed);
            state.wanted = None;
            return None;
        }
        if state.wanted.as_ref() != Some(&key) {
            self.cancel.store(true, Ordering::Relaxed);
            state.wanted = None;
            if start {
                self.cancel = Arc::new(AtomicBool::new(false));
                state.wanted = Some(key.clone());
                let request = Request {
                    key,
                    image: Arc::clone(image),
                    cancel: Arc::clone(&self.cancel),
                };
                if self
                    .sender
                    .as_ref()
                    .is_none_or(|sender| sender.send(request).is_err())
                {
                    state.wanted = None;
                    return Some(Err("BM3D worker is unavailable".into()));
                }
            }
        }
        None
    }
    /// 解码源变化时释放旧结果；其 source 代号已失效，不再有复用价值。
    pub fn clear(&mut self) {
        self.cancel();
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ready = None;
    }

    /// 快速档、换照片或退出编辑时取消在途工作；已完成的一份仍可在切回时复用。
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .wanted = None;
    }
}
impl<T: DenoiseImage> Drop for BackgroundDenoise<T> {
    fn drop(&mut self) {
        self.cancel();
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::lens::ManualLens;
    use super::*;
    use std::time::Duration;
    #[test]
    fn floating_source_uses_the_same_cancellable_worker_without_clipping() {
        let working=crate::color::working::WorkingImage::from_linear_srgb(2,1,vec![[-0.1,0.2,1.2];2]).unwrap();
        let image=Arc::new(super::super::working::WorkingReferenceImage::from_working(working).unwrap());
        let (sender,receiver)=std::sync::mpsc::channel();
        let mut worker=WorkingHighDenoise::new(move || {let _=sender.send(());});
        let mut wanted=key(4001); wanted.width=2;wanted.height=1;wanted.plan=DenoisePlan::default();
        wanted.lens=LensCorrection::manual_only(2,1,Default::default());
        assert!(worker.get_or_request(wanted.clone(),&image,true).is_none());
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        let actual=worker.get_or_request(wanted.clone(),&image,false).unwrap().unwrap();
        for (a,b) in actual.view().rgb.iter().zip(image.view().rgb) {assert!((a-b).abs()<0.000001);}
        assert!(actual.view().rgb[0]<0.0 && actual.view().rgb[2]>1.0);
        worker.clear();
        assert!(worker.get_or_request(wanted,&image,false).is_none());
    }
    fn key(source: u64) -> DenoiseKey {
        DenoiseKey {
            source,
            width: 9,
            height: 9,
            plan: DenoisePlan {
                luma: 0.5,
                chroma: 0.5,
            },
            lens: LensCorrection::manual_only(9, 9, ManualLens::default()),
        }
    }
    #[test]
    fn interactive_requests_do_not_start_work_and_ready_result_is_reused() {
        let (tx, rx) = mpsc::channel();
        let mut worker = HighDenoise::new(move || {
            let _ = tx.send(());
        });
        let image = Arc::new(LinearImage {
            width: 9,
            height: 9,
            rgb: vec![25000; 9 * 9 * 3],
        });
        assert!(worker.get_or_request(key(1), &image, false).is_none());
        assert!(worker.shared.lock().unwrap().wanted.is_none());
        assert!(worker.get_or_request(key(1), &image, true).is_none());
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let first = worker
            .get_or_request(key(1), &image, true)
            .unwrap()
            .unwrap();
        let second = worker
            .get_or_request(key(1), &image, false)
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let mut changed = key(1);
        changed.lens.manual.distortion = 0.5;
        assert!(worker.get_or_request(changed, &image, false).is_none());
        assert!(worker.get_or_request(key(2), &image, false).is_none());
        let mut changed = key(1);
        changed.plan.chroma = 0.7;
        assert!(worker.get_or_request(changed, &image, false).is_none());
    }
    #[test]
    fn obsolete_completion_cannot_replace_latest_requested_source() {
        let (tx, rx) = mpsc::channel();
        let mut worker = HighDenoise::new(move || {
            let _ = tx.send(());
        });
        let image = Arc::new(LinearImage {
            width: 9,
            height: 9,
            rgb: vec![25000; 9 * 9 * 3],
        });
        worker.get_or_request(key(1), &image, true);
        worker.get_or_request(key(2), &image, true);
        // Notification may race with the second request; the keyed cache remains authoritative.
        for _ in 0..2 {
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
            if worker.get_or_request(key(2), &image, true).is_some() {
                break;
            }
        }
        assert!(worker.get_or_request(key(2), &image, true).is_some());
        assert!(worker.get_or_request(key(1), &image, false).is_none());
    }
}
