//! One active upload and one replaceable latest request. The render thread only
//! polls completed textures; no CPU packing or GPU wait runs on that thread.
use super::RenderImage;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

struct Mailbox<I, O> {
    next: Mutex<Option<(u64, I)>>,
    result: Mutex<Option<(u64, O)>>,
    wake: Condvar,
    generation: AtomicU64,
    stopped: AtomicBool,
}
struct LatestWorker<I, O> {
    shared: Arc<Mailbox<I, O>>,
}
impl<I: Send + 'static, O: Send + 'static> LatestWorker<I, O> {
    fn new(work: impl Fn(I, &dyn Fn() -> bool) -> O + Send + 'static) -> Self {
        let shared = Arc::new(Mailbox {
            next: Mutex::new(None),
            result: Mutex::new(None),
            wake: Condvar::new(),
            generation: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("raybend-gpu-upload".into())
            .spawn(move || {
                loop {
                    let mut next = worker.next.lock().unwrap_or_else(|e| e.into_inner());
                    while next.is_none() && !worker.stopped.load(Ordering::Acquire) {
                        next = worker.wake.wait(next).unwrap_or_else(|e| e.into_inner());
                    }
                    if worker.stopped.load(Ordering::Acquire) {
                        break;
                    }
                    let (generation, input) = next.take().expect("pending upload");
                    drop(next);
                    let cancelled = || {
                        worker.stopped.load(Ordering::Acquire)
                            || worker.generation.load(Ordering::Acquire) != generation
                    };
                    let output = work(input, &cancelled);
                    if !cancelled() {
                        let mut result = worker.result.lock().unwrap_or_else(|e| e.into_inner());
                        if !cancelled() {
                            *result = Some((generation, output));
                        }
                    }
                }
            })
            .expect("upload worker thread");
        Self { shared }
    }
    fn cancel(&self) {
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared
            .next
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.shared
            .result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
    fn request(&self, input: I) {
        self.cancel();
        let generation = self.shared.generation.load(Ordering::Acquire);
        *self.shared.next.lock().unwrap_or_else(|e| e.into_inner()) = Some((generation, input));
        self.shared.wake.notify_one();
    }
    fn take(&self) -> Option<O> {
        self.shared
            .result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .filter(|(generation, _)| *generation == self.shared.generation.load(Ordering::Acquire))
            .map(|(_, v)| v)
    }
}
impl<I, O> Drop for LatestWorker<I, O> {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared
            .next
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.shared
            .result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.shared.wake.notify_one();
        // Detached thread sees cancellation between tiles (GPU waits are bounded).
        // Never join a driver wait from the render thread or window destructor.
    }
}

type Textures = Vec<(Arc<RenderImage>, wgpu::Texture)>;
struct Request {
    device: wgpu::Device,
    queue: wgpu::Queue,
    images: Vec<Arc<RenderImage>>,
}
pub(super) struct ImageUploader {
    worker: LatestWorker<Request, Result<Textures, String>>,
    desired: Vec<Arc<RenderImage>>,
    ready: Textures,
    pending: bool,
}
fn same(a: &Arc<RenderImage>, b: &Arc<RenderImage>) -> bool {
    Arc::ptr_eq(a, b) || a.shares_source(b)
}
impl Default for ImageUploader {
    fn default() -> Self {
        Self {
            worker: LatestWorker::new(|request: Request, cancelled| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut textures = Vec::with_capacity(request.images.len());
                    for image in request.images {
                        if cancelled() {
                            return Err("GPU source upload cancelled".into());
                        }
                        let (texture, error) = super::gpu::create_image_texture_cancellable(
                            &request.device,
                            &request.queue,
                            &image,
                            "prepared-frame",
                            cancelled,
                        );
                        if let Some(error) = error {
                            return Err(error);
                        }
                        textures.push((image, texture));
                    }
                    Ok(textures)
                }))
                .unwrap_or_else(|_| Err("GPU source upload worker panicked".into()))
            }),
            desired: Vec::new(),
            ready: Vec::new(),
            pending: false,
        }
    }
}
impl ImageUploader {
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: Vec<Arc<RenderImage>>,
    ) -> Result<bool, String> {
        if images.len() != self.desired.len()
            || images.iter().zip(&self.desired).any(|(a, b)| !same(a, b))
        {
            self.ready.clear();
            self.desired = images.clone();
            self.worker.request(Request {
                device: device.clone(),
                queue: queue.clone(),
                images,
            });
            self.pending = true;
        }
        if let Some(result) = self.worker.take() {
            self.pending = false;
            match result {
                Ok(ready) => self.ready = ready,
                Err(error) => {
                    self.desired.clear();
                    return Err(error);
                }
            }
        }
        Ok(!self.pending)
    }
    pub fn texture(&self, image: &Arc<RenderImage>) -> Option<wgpu::Texture> {
        self.ready
            .iter()
            .find(|(source, _)| same(source, image))
            .map(|(_, texture)| texture.clone())
    }
    pub fn cancel(&mut self) {
        self.worker.cancel();
        self.desired.clear();
        self.ready.clear();
        self.pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_is_bounded_cancels_active_and_never_delivers_stale_result() {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = LatestWorker::new(move |value, cancelled| {
            entered_tx.send(value).unwrap();
            release_rx.recv().unwrap();
            (value, cancelled())
        });
        worker.request(1);
        assert_eq!(entered_rx.recv().unwrap(), 1);
        for value in 2..100 {
            worker.request(value);
        }
        release_tx.send(()).unwrap();
        assert_eq!(entered_rx.recv().unwrap(), 99);
        assert!(worker.take().is_none());
        release_tx.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(result) = worker.take() {
                assert_eq!(result, (99, false));
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        worker.request(100);
        assert_eq!(entered_rx.recv().unwrap(), 100);
        worker.cancel();
        release_tx.send(()).unwrap();
        assert!(worker.take().is_none());
    }
}
