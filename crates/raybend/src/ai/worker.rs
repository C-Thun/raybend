//! 独立 AI 进程，复用 RAW 的进程/有界帧/超时监督；一次一张，不传像素到 WebView。
use crate::worker_process::{self, Proc, WorkerError};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
pub use super::WORKER_ARG;
pub const VERSION: u32 = 1;
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    op: String,
    library: Option<PathBuf>,
    model: Option<PathBuf>,
    path: Option<PathBuf>,
    raw: bool,
    resize: Option<super::preprocess::ResizePolicy>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    version: u32,
    ok: bool,
    error: Option<String>,
    features: Vec<f32>,
}
fn read<R: BufRead>(reader: &mut R) -> Result<Response, WorkerError> {
    let bytes = worker_process::read_frame(reader)?;
    let result: Response =
        serde_json::from_slice(&bytes).map_err(|e| WorkerError::Protocol(e.to_string()))?;
    if result.version != VERSION
        || result.features.len() > 512
        || result.features.iter().any(|v| !v.is_finite())
        || result.ok && result.error.is_some()
        || !result.ok && !result.features.is_empty()
    {
        return Err(WorkerError::Protocol("AI 响应身份或特征无效".into()));
    }
    Ok(result)
}
/// 应用退出时可同步终止独立子进程；不等待推理或持有数据库锁。
#[derive(Clone, Default)]
pub struct Shutdown {
    stopped: Arc<AtomicBool>,
    process: Arc<Mutex<Option<Arc<Mutex<Proc>>>>>,
}
impl Shutdown {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        if let Ok(slot) = self.process.lock() {
            if let Some(proc) = slot.as_ref() {
                worker_process::kill_proc(proc);
            }
        }
    }
    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
}
pub struct Worker {
    process: Option<Arc<Mutex<Proc>>>,
    bin: PathBuf,
    library: PathBuf,
    model: PathBuf,
    shutdown: Shutdown,
}
impl Worker {
    pub fn new(bin: PathBuf, library: PathBuf, model: PathBuf) -> Self {
        Self::with_shutdown(bin, library, model, Shutdown::default())
    }
    pub fn with_shutdown(
        bin: PathBuf,
        library: PathBuf,
        model: PathBuf,
        shutdown: Shutdown,
    ) -> Self {
        Self {
            process: None,
            bin,
            library,
            model,
            shutdown,
        }
    }
    fn exchange(&mut self, request: Request) -> Result<Response, WorkerError> {
        if self.shutdown.stopped() {
            return Err(WorkerError::Crashed("AI worker 已终止".into()));
        }
        if self.process.is_none() {
            let proc = Arc::new(Mutex::new(worker_process::spawn_proc(
                &self.bin,
                &[WORKER_ARG.into()],
            )?));
            {
                let mut slot = self
                    .shutdown
                    .process
                    .lock()
                    .map_err(|_| WorkerError::Crashed("AI 退出锁损坏".into()))?;
                if self.shutdown.stopped() {
                    worker_process::kill_proc(&proc);
                    return Err(WorkerError::Crashed("AI worker 已终止".into()));
                }
                *slot = Some(proc.clone());
            }
            let init = Request {
                version: VERSION,
                op: "load".into(),
                library: Some(self.library.clone()),
                model: Some(self.model.clone()),
                ..Default::default()
            };
            let bytes =
                serde_json::to_vec(&init).map_err(|e| WorkerError::Protocol(e.to_string()))?;
            match worker_process::exchange(&proc, &bytes, Duration::from_secs(30), read) {
                Ok(r) if r.ok => self.process = Some(proc),
                Ok(r) => {
                    worker_process::kill_proc(&proc);
                    return Err(WorkerError::Decode(
                        r.error.unwrap_or_else(|| "AI 模型载入失败".into()),
                    ));
                }
                Err(e) => {
                    worker_process::kill_proc(&proc);
                    return Err(e);
                }
            }
        }
        let bytes =
            serde_json::to_vec(&request).map_err(|e| WorkerError::Protocol(e.to_string()))?;
        let response = worker_process::exchange(
            self.process.as_ref().unwrap(),
            &bytes,
            Duration::from_secs(30),
            read,
        );
        if response.as_ref().is_err_and(|e| e.needs_respawn()) {
            self.shutdown();
        }
        let response = response?;
        if response.ok {
            Ok(response)
        } else {
            Err(WorkerError::Decode(
                response.error.unwrap_or_else(|| "AI 识别失败".into()),
            ))
        }
    }
    pub fn probe(&mut self) -> Result<(), WorkerError> {
        self.exchange(Request {
            version: VERSION,
            op: "probe".into(),
            ..Default::default()
        })
        .map(|_| ())
    }
    pub fn encode(
        &mut self,
        path: &Path,
        raw: bool,
        resize: super::preprocess::ResizePolicy,
    ) -> Result<Vec<f32>, WorkerError> {
        let r = self.exchange(Request {
            version: VERSION,
            op: "encode".into(),
            path: Some(path.into()),
            raw,
            resize: Some(resize),
            ..Default::default()
        })?;
        if r.features.len() != 512 {
            self.shutdown();
            return Err(WorkerError::Protocol("AI 特征数量无效".into()));
        }
        Ok(r.features)
    }
    pub fn shutdown(&mut self) {
        if let Some(proc) = self.process.take() {
            worker_process::kill_proc(&proc);
            if let Ok(mut slot) = self.shutdown.process.lock() {
                if slot
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &proc))
                {
                    slot.take();
                }
            }
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}
pub fn run_worker_main() -> i32 {
    eprintln!("[ai-worker] raybend-ai-proto-v1 CPU-only");
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    let mut encoder = None;
    loop {
        let bytes = match worker_process::read_frame(&mut reader) {
            Ok(b) => b,
            Err(WorkerError::Crashed(_)) => return 0,
            Err(_) => return 2,
        };
        let result = (|| -> crate::Result<Vec<f32>> {
            let req: Request = serde_json::from_slice(&bytes)?;
            if req.version != VERSION {
                return Err(crate::Error::Unsupported("AI worker 协议版本不匹配".into()));
            }
            match req.op.as_str() {
                "load" => {
                    encoder = Some(super::runtime::Encoder::load(
                        req.library
                            .as_deref()
                            .ok_or_else(|| crate::Error::Unsupported("缺运行库".into()))?,
                        req.model
                            .as_deref()
                            .ok_or_else(|| crate::Error::Unsupported("缺模型".into()))?,
                    )?);
                    Ok(Vec::new())
                }
                "probe" => {
                    if encoder.is_none() {
                        return Err(crate::Error::Unsupported("模型尚未载入".into()));
                    }
                    Ok(Vec::new())
                }
                "encode" => {
                    let path = req
                        .path
                        .ok_or_else(|| crate::Error::Unsupported("缺照片路径".into()))?;
                    let rgb = super::preprocess::original_path(&path, req.raw)?;
                    let pixels = super::preprocess::tensor(
                        &rgb,
                        req.resize
                            .ok_or_else(|| crate::Error::Unsupported("缺预处理策略".into()))?,
                    )?;
                    encoder
                        .as_mut()
                        .ok_or_else(|| crate::Error::Unsupported("模型尚未载入".into()))?
                        .encode(pixels)
                }
                _ => Err(crate::Error::Unsupported("AI worker 操作无效".into())),
            }
        })();
        let response = match result {
            Ok(features) => Response {
                version: VERSION,
                ok: true,
                features,
                error: None,
            },
            Err(e) => Response {
                version: VERSION,
                ok: false,
                error: Some(e.to_string()),
                features: Vec::new(),
            },
        };
        let Ok(bytes) = serde_json::to_vec(&response) else {
            return 2;
        };
        if worker_process::write_frame(&mut writer, &bytes).is_err() {
            return 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(value: serde_json::Value) -> Vec<u8> {
        let bytes = serde_json::to_vec(&value).unwrap();
        let mut out = (bytes.len() as u32).to_le_bytes().to_vec();
        out.extend(bytes);
        out
    }
    #[cfg(unix)]
    #[test]
    fn application_exit_interrupts_blocked_worker_and_reaps_child() {
        let proc = Arc::new(Mutex::new(
            worker_process::spawn_proc(
                Path::new("/bin/sh"),
                &["-c".into(), "exec sleep 10".into()],
            )
            .unwrap(),
        ));
        let shutdown = Shutdown::default();
        *shutdown.process.lock().unwrap() = Some(proc.clone());
        let terminate = shutdown.clone();
        let started = std::time::Instant::now();
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            terminate.stop();
        });
        let result = worker_process::exchange(&proc, b"{}", Duration::from_secs(5), read);
        stopper.join().unwrap();
        assert!(result.is_err());
        assert!(shutdown.stopped());
        assert!(proc.lock().unwrap().child.try_wait().unwrap().is_some());
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn bounded_response_rejects_version_unknown_fields_shape_and_contradictions() {
        for value in [
            serde_json::json!({"version":2,"ok":true,"error":null,"features":[]}),
            serde_json::json!({"version":1,"ok":true,"error":null,"features":vec![0.;513]}),
            serde_json::json!({"version":1,"ok":true,"error":"failure","features":[]}),
            serde_json::json!({"version":1,"ok":true,"error":null,"features":[],"extra":0}),
        ] {
            assert!(read(&mut std::io::Cursor::new(frame(value))).is_err());
        }
        assert!(read(&mut std::io::Cursor::new((65537_u32).to_le_bytes())).is_err());
    }
}
