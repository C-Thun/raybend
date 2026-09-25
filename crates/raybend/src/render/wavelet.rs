//! Shared headless wgpu compute service for editor and thumbnails. No window lifetime dependency.
//! Four undecimated B3-spline scales; sum of radii = 2*(1+2+4+8) = 30 pixels.
//! Tiles retain that complete halo, so their interiors equal whole-image processing.
use crate::develop::{denoise::DenoisePlan, pipeline::LinearImage};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use wgpu::util::DeviceExt;
const TILE: usize = 512;
const HALO: usize = 30;
const SCALES: [f32; 4] = [0.8908, 0.2007, 0.0856, 0.0412];

/// One bounded device/workspace shared across callers. A broken device falls back to CPU until restart.
/// Tiny images avoid device initialization and transfer overhead entirely.
pub fn try_denoise(source: &LinearImage, plan: &DenoisePlan) -> Option<LinearImage> {
    if !source.is_consistent() || plan.is_identity() || source.rgb.len() < 64 * 64 * 3 {
        return None;
    }
    static ENGINE: OnceLock<Mutex<Result<WaveletGpu, String>>> = OnceLock::new();
    let mut state = ENGINE
        .get_or_init(|| Mutex::new(WaveletGpu::new(false)))
        .lock()
        .ok()?;
    let result = match state.as_mut() {
        Ok(engine) => engine.denoise(source, plan),
        Err(_) => return None,
    };
    match result {
        Ok(image) => Some(image),
        Err(error) => {
            eprintln!("[denoise] GPU wavelet unavailable; CPU fallback: {error}");
            *state = Err(error);
            None
        }
    }
}

pub struct WaveletGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    pipelines: Vec<wgpu::ComputePipeline>,
    broken: Arc<AtomicBool>,
    adapter: String,
}
impl WaveletGpu {
    /// `allow_software` is for explicit shader smoke tests, never the production default.
    pub fn new(allow_software: bool) -> Result<Self, String> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| e.to_string())?;
        let info = adapter.get_info();
        if !allow_software && info.device_type == wgpu::DeviceType::Cpu {
            return Err("software adapter".into());
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("raybend-wavelet"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let broken = Arc::new(AtomicBool::new(false));
        let lost = Arc::clone(&broken);
        device.set_device_lost_callback(move |_, _| {
            lost.store(true, Ordering::Relaxed);
        });
        let invalid = Arc::clone(&broken);
        device.on_uncaptured_error(Arc::new(move |error| {
            invalid.store(true, Ordering::Relaxed);
            eprintln!("[denoise] wgpu: {error}");
        }));
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let entries: Vec<_> = (0..6)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 0 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage { read_only: false }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wavelet"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wavelet"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wavelet"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wavelet.wgsl").into()),
        });
        let pipelines = ["initialize", "horizontal", "vertical", "shrink", "finish"]
            .into_iter()
            .map(|entry| {
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&pipeline_layout),
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
            })
            .collect();
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(error.to_string());
        }
        let adapter = format!("{} / {:?}", info.name, info.backend);
        eprintln!("[denoise] GPU wavelet: {adapter}");
        Ok(Self {
            device,
            queue,
            layout,
            pipelines,
            broken,
            adapter,
        })
    }
    pub fn adapter_name(&self) -> &str {
        &self.adapter
    }
    pub fn denoise(
        &mut self,
        source: &LinearImage,
        plan: &DenoisePlan,
    ) -> Result<LinearImage, String> {
        self.run(source, plan, TILE)
    }
    fn run(
        &mut self,
        source: &LinearImage,
        plan: &DenoisePlan,
        tile: usize,
    ) -> Result<LinearImage, String> {
        if !source.is_consistent() {
            return Err("inconsistent linear image".into());
        }
        if plan.is_identity() {
            return Ok(source.clone());
        }
        if self.broken.load(Ordering::Relaxed) {
            return Err("device lost".into());
        }
        let strength = |x: f32| {
            if x.is_finite() {
                x.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let width = source.width as usize;
        let height = source.height as usize;
        let capacity = (tile + 2 * HALO).min(width) * (tile + 2 * HALO).min(height);
        let buffer = |size, usage| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wavelet tile"),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let pixels = buffer(
            capacity as u64 * 8,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let readback = buffer(
            capacity as u64 * 8,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        let planes: Vec<_> = (0..4)
            .map(|_| buffer(capacity as u64 * 16, wgpu::BufferUsages::STORAGE))
            .collect();
        let mut out = source.clone();
        for y in (0..height).step_by(tile) {
            for x in (0..width).step_by(tile) {
                let x0 = x.saturating_sub(HALO);
                let y0 = y.saturating_sub(HALO);
                let x1 = (x + tile + HALO).min(width);
                let y1 = (y + tile + HALO).min(height);
                let w = x1 - x0;
                let h = y1 - y0;
                let mut bytes = Vec::with_capacity(w * h * 8);
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let i = (yy * width + xx) * 3;
                        let rgb = &source.rgb[i..i + 3];
                        bytes.extend_from_slice(
                            &(u32::from(rgb[0]) | u32::from(rgb[1]) << 16).to_le_bytes(),
                        );
                        bytes.extend_from_slice(&u32::from(rgb[2]).to_le_bytes());
                    }
                }
                self.queue.write_buffer(&pixels, 0, &bytes);
                let mut groups = Vec::new();
                for (level, scale) in SCALES.into_iter().enumerate() {
                    let mut data = Vec::new();
                    for v in [
                        w as u32,
                        h as u32,
                        1 << level,
                        scale.to_bits(),
                        strength(plan.luma).to_bits(),
                        strength(plan.chroma).to_bits(),
                        0,
                        0,
                    ] {
                        data.extend_from_slice(&v.to_le_bytes());
                    }
                    let uniform =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("wavelet parameters"),
                                contents: &data,
                                usage: wgpu::BufferUsages::UNIFORM,
                            });
                    let a = level % 2;
                    let b = 1 - a;
                    let buffers = [
                        &uniform, &pixels, &planes[a], &planes[2], &planes[b], &planes[3],
                    ];
                    let entries: Vec<_> = buffers
                        .iter()
                        .enumerate()
                        .map(|(binding, buffer)| wgpu::BindGroupEntry {
                            binding: binding as u32,
                            resource: buffer.as_entire_binding(),
                        })
                        .collect();
                    groups.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("wavelet"),
                        layout: &self.layout,
                        entries: &entries,
                    }));
                }
                let mut encoder = self.device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("wavelet"),
                        timestamp_writes: None,
                    });
                    pass.set_bind_group(0, &groups[0], &[]);
                    pass.set_pipeline(&self.pipelines[0]);
                    pass.dispatch_workgroups((w as u32).div_ceil(16), (h as u32).div_ceil(16), 1);
                    for group in &groups {
                        pass.set_bind_group(0, group, &[]);
                        for pipeline in &self.pipelines[1..4] {
                            pass.set_pipeline(pipeline);
                            pass.dispatch_workgroups(
                                (w as u32).div_ceil(16),
                                (h as u32).div_ceil(16),
                                1,
                            );
                        }
                    }
                    pass.set_pipeline(&self.pipelines[4]);
                    pass.dispatch_workgroups((w as u32).div_ceil(16), (h as u32).div_ceil(16), 1);
                }
                encoder.copy_buffer_to_buffer(&pixels, 0, &readback, 0, (w * h * 8) as u64);
                let submission = self.queue.submit(Some(encoder.finish()));
                let slice = readback.slice(..(w * h * 8) as u64);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
                self.device
                    .poll(wgpu::PollType::Wait {
                        submission_index: Some(submission),
                        timeout: Some(Duration::from_secs(10)),
                    })
                    .map_err(|e| e.to_string())?;
                rx.recv_timeout(Duration::from_secs(1))
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?;
                let mapped = slice.get_mapped_range().map_err(|e| e.to_string())?;
                for yy in y..(y + tile).min(height) {
                    for xx in x..(x + tile).min(width) {
                        let src = ((yy - y0) * w + xx - x0) * 8;
                        let dst = (yy * width + xx) * 3;
                        for (channel, offset) in [0, 2, 4].into_iter().enumerate() {
                            out.rgb[dst + channel] = u16::from_le_bytes([
                                mapped[src + offset],
                                mapped[src + offset + 1],
                            ]);
                        }
                    }
                }
                drop(mapped);
                readback.unmap();
                if self.broken.load(Ordering::Relaxed) {
                    return Err("device lost during compute".into());
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(w: u32, h: u32) -> LinearImage {
        let mut seed = 5_u32;
        let rgb = (0..w * h)
            .flat_map(|i| {
                let base = 16000 + (i % w) * 12000 / w + (i / w) * 10000 / h;
                std::array::from_fn::<_, 3, _>(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (base + (seed >> 16) % 2400) as u16
                })
            })
            .collect();
        LinearImage {
            width: w,
            height: h,
            rgb,
        }
    }
    // Deliberately simple whole-frame scalar oracle: independent from GPU dispatch and tile packing.
    fn oracle(source: &LinearImage, plan: &DenoisePlan) -> LinearImage {
        let (w, h) = (source.width as usize, source.height as usize);
        let original: Vec<[f32; 3]> = source
            .rgb
            .chunks_exact(3)
            .map(|rgb| {
                let r = f32::from(rgb[0]) / 65535.0;
                let g = f32::from(rgb[1]) / 65535.0;
                let b = f32::from(rgb[2]) / 65535.0;
                let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                [y, b - y, r - y]
            })
            .collect();
        let mut low = original.clone();
        let mut removed = vec![[0.0; 3]; w * h];
        for (level, scale) in SCALES.into_iter().enumerate() {
            let step = 1_isize << level;
            let mut horizontal = vec![[0.0; 3]; w * h];
            let mut next = horizontal.clone();
            {
                for y in 0..h {
                    for x in 0..w {
                        for c in 0..3 {
                            horizontal[y * w + x][c] = [-2, -1, 0, 1, 2]
                                .into_iter()
                                .zip([1.0, 4.0, 6.0, 4.0, 1.0])
                                .map(|(k, v)| {
                                    let xx =
                                        (x as isize + k * step).clamp(0, w as isize - 1) as usize;
                                    low[y * w + xx][c] * v / 16.0
                                })
                                .sum();
                        }
                    }
                }
            }
            for y in 0..h {
                for x in 0..w {
                    for c in 0..3 {
                        next[y * w + x][c] = [-2, -1, 0, 1, 2]
                            .into_iter()
                            .zip([1.0, 4.0, 6.0, 4.0, 1.0])
                            .map(|(k, v)| {
                                let yy = (y as isize + k * step).clamp(0, h as isize - 1) as usize;
                                horizontal[yy * w + x][c] * v / 16.0
                            })
                            .sum();
                        let d = low[y * w + x][c] - next[y * w + x][c];
                        let t = if c == 0 {
                            plan.luma * 0.035
                        } else {
                            plan.chroma * 0.05
                        } * scale;
                        removed[y * w + x][c] +=
                            d.signum() * d.abs().min((2.0 * t - d.abs()).max(0.0));
                    }
                }
            }
            low = next;
        }
        let rgb = original
            .iter()
            .zip(removed)
            .flat_map(|(v, d)| {
                let y = v[0] - d[0];
                let r = y + v[2] - d[2];
                let b = y + v[1] - d[1];
                let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
                [r, g, b].map(|c| (c.clamp(0.0, 1.0) * 65535.0).round() as u16)
            })
            .collect();
        LinearImage {
            width: source.width,
            height: source.height,
            rgb,
        }
    }
    #[test]
    fn identity_tiny_and_malformed_do_not_initialize_a_gpu() {
        assert!(try_denoise(&sample(100, 100), &DenoisePlan::default()).is_none());
        assert!(
            try_denoise(
                &sample(1, 1),
                &DenoisePlan {
                    luma: 1.0,
                    chroma: 1.0
                }
            )
            .is_none()
        );
        assert!(
            try_denoise(
                &LinearImage {
                    width: 100,
                    height: 100,
                    rgb: vec![0; 3]
                },
                &DenoisePlan {
                    luma: 1.0,
                    chroma: 1.0
                }
            )
            .is_none()
        );
        assert_eq!(
            HALO,
            (0..SCALES.len())
                .map(|level| 2 * (1 << level))
                .sum::<usize>()
        );
    }
    #[test]
    #[ignore = "explicit headless GPU shader/transfer smoke; needs a wgpu adapter"]
    fn gpu_matches_scalar_and_tile_interiors() {
        let mut gpu = WaveletGpu::new(true).expect("wgpu adapter and valid shader");
        eprintln!("adapter: {}", gpu.adapter_name());
        for (w, h) in [
            (1, 1),
            (1, 71),
            (73, 1),
            (3, 5),
            (37, 43),
            (139, 131),
            (517, 9),
        ] {
            let source = sample(w, h);
            for plan in [
                DenoisePlan {
                    luma: 0.6,
                    chroma: 0.8,
                },
                DenoisePlan {
                    luma: 0.0,
                    chroma: 1.0,
                },
                DenoisePlan {
                    luma: 1.0,
                    chroma: 0.0,
                },
            ] {
                let actual = gpu.run(&source, &plan, 64).unwrap();
                let expected = oracle(&source, &plan);
                let max = actual
                    .rgb
                    .iter()
                    .zip(expected.rgb)
                    .map(|(a, b)| a.abs_diff(b))
                    .max()
                    .unwrap();
                assert!(max <= 2, "{w}x{h}, {plan:?}: scalar difference {max}");
                if plan.luma == 0.0 {
                    for (a, b) in actual.rgb.chunks_exact(3).zip(source.rgb.chunks_exact(3)) {
                        let y = |c: &[u16]| {
                            0.2126 * f32::from(c[0])
                                + 0.7152 * f32::from(c[1])
                                + 0.0722 * f32::from(c[2])
                        };
                        assert!((y(a) - y(b)).abs() < 1.1, "chroma must preserve luma");
                    }
                }
            }
        }
        let flat = LinearImage {
            width: 133,
            height: 71,
            rgb: vec![30000; 133 * 71 * 3],
        };
        let plan = DenoisePlan {
            luma: 1.0,
            chroma: 1.0,
        };
        assert_eq!(
            gpu.denoise(&flat, &plan).unwrap().rgb,
            flat.rgb,
            "no flat-field/border bias"
        );
        let mut noisy = flat.clone();
        for (i, pixel) in noisy.rgb.chunks_exact_mut(3).enumerate() {
            pixel.fill((30000_i32 + ((i * 7919) % 1601) as i32 - 800) as u16);
        }
        let result = gpu.denoise(&noisy, &plan).unwrap();
        let mse = |rgb: &[u16]| {
            rgb.iter()
                .map(|v| (f64::from(*v) - 30000.0).powi(2))
                .sum::<f64>()
        };
        assert!(
            mse(&result.rgb) < mse(&noisy.rgb) * 0.4,
            "noise energy must fall"
        );
        let near = gpu
            .denoise(
                &noisy,
                &DenoisePlan {
                    luma: 1e-6,
                    chroma: 1e-6,
                },
            )
            .unwrap();
        assert!(
            near.rgb
                .iter()
                .zip(&noisy.rgb)
                .all(|(a, b)| a.abs_diff(*b) <= 1),
            "strength continuous at zero"
        );
        gpu.device.destroy();
        let _ = gpu.device.poll(wgpu::PollType::Poll);
        assert!(
            gpu.denoise(&noisy, &plan).is_err(),
            "lost device falls back instead of publishing invalid pixels"
        );
    }
}
