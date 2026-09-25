//! 编辑工具的矢量覆盖层，与照片同一个 render pass / present。
//! 几何仍取自 Viewport；不回读像素、不经过 WebView 的状态轮询。

use super::{ClipRect, Srgb8, Viewport};
use crate::develop::geometry::CropRect;

#[derive(Debug, Clone, Copy)]
pub struct OverlayPalette {
    pub line: Srgb8,
    pub halo: Srgb8,
    pub crop: Srgb8,
    pub rotate: Srgb8,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolOverlay {
    pub crop: CropRect,
    pub source: (u32, u32),
    pub handles: bool,
    /// 原始窗口 CSS 坐标，DPR 只在构造顶点时乘一次。
    pub straighten: Option<[(f32, f32); 2]>,
}

#[derive(Default)]
struct Mesh {
    vertices: Vec<[f32; 6]>,
}

impl Mesh {
    fn quad(&mut self, points: [(f32, f32); 4], color: [f32; 4]) {
        for i in [0, 1, 2, 0, 2, 3] {
            let (x, y) = points[i];
            self.vertices
                .push([x, y, color[0], color[1], color[2], color[3]]);
        }
    }

    fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        self.quad(
            [
                (x, y),
                (x + width, y),
                (x + width, y + height),
                (x, y + height),
            ],
            color,
        );
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, color: [f32; 4]) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = dx.hypot(dy);
        if length < f32::EPSILON {
            return;
        }
        let (nx, ny) = (-dy / length * width / 2.0, dx / length * width / 2.0);
        self.quad(
            [
                (a.0 + nx, a.1 + ny),
                (b.0 + nx, b.1 + ny),
                (b.0 - nx, b.1 - ny),
                (a.0 - nx, a.1 - ny),
            ],
            color,
        );
    }

    fn outline(&mut self, rect: ClipRect, width: f32, color: [f32; 4]) {
        let ClipRect {
            x,
            y,
            width: w,
            height: h,
        } = rect;
        let d = width / 2.0;
        self.rect(x - d, y - d, w + width, width, color);
        self.rect(x - d, y + h - d, w + width, width, color);
        self.rect(x - d, y + d, width, (h - width).max(0.0), color);
        self.rect(x + w - d, y + d, width, (h - width).max(0.0), color);
    }
}

fn rgba(color: Srgb8, alpha: f32) -> [f32; 4] {
    let linear = color.to_clear_color();
    [linear.r as f32, linear.g as f32, linear.b as f32, alpha]
}

/// 同一份 Viewport 决定照片、裁切框和对比分割点；输出为物理像素顶点。
fn mesh(
    viewport: &Viewport,
    tool: Option<ToolOverlay>,
    compare: Option<f32>,
    palette: OverlayPalette,
) -> Mesh {
    let mut mesh = Mesh::default();
    if viewport.scissor().is_none() || !viewport.dpr.is_finite() || viewport.dpr <= 0.0 {
        return mesh;
    }
    let hole = viewport.effective_rect();
    let d = viewport.dpr;
    let line = rgba(palette.line, 1.0);
    let halo = rgba(palette.halo, 1.0);
    if let Some(tool) = tool {
        if let Some(css) = viewport.frame_rect_css(tool.crop, tool.source) {
            let rect = ClipRect {
                x: hole.x + css.x * d,
                y: hole.y + css.y * d,
                width: css.width * d,
                height: css.height * d,
            };
            let ClipRect {
                x,
                y,
                width: w,
                height: h,
            } = rect;
            // 四块遮罩，避免超大 box-shadow 引起 WebView 全视口重绘。
            let mask = rgba(palette.halo, 0.42);
            mesh.rect(hole.x, hole.y, hole.width, y - hole.y, mask);
            mesh.rect(
                hole.x,
                y + h,
                hole.width,
                hole.y + hole.height - y - h,
                mask,
            );
            mesh.rect(hole.x, y, x - hole.x, h, mask);
            mesh.rect(x + w, y, hole.x + hole.width - x - w, h, mask);
            mesh.outline(rect, 4.0 * d, halo);
            mesh.outline(rect, 2.0 * d, line);
            let grid = rgba(
                if tool.handles {
                    palette.crop
                } else {
                    palette.rotate
                },
                1.0,
            );
            for fraction in [1.0 / 3.0, 2.0 / 3.0] {
                mesh.line((x + w * fraction, y), (x + w * fraction, y + h), d, grid);
                mesh.line((x, y + h * fraction), (x + w, y + h * fraction), d, grid);
            }
            if tool.handles {
                for (fx, fy) in [
                    (0.0, 0.0),
                    (0.5, 0.0),
                    (1.0, 0.0),
                    (1.0, 0.5),
                    (1.0, 1.0),
                    (0.5, 1.0),
                    (0.0, 1.0),
                    (0.0, 0.5),
                ] {
                    let (cx, cy) = (x + w * fx, y + h * fy);
                    mesh.rect(cx - 5.0 * d, cy - 5.0 * d, 10.0 * d, 10.0 * d, halo);
                    mesh.rect(cx - 4.0 * d, cy - 4.0 * d, 8.0 * d, 8.0 * d, line);
                }
            }
        }
        if let Some([a, b]) = tool.straighten {
            let a = (a.0 * d, a.1 * d);
            let b = (b.0 * d, b.1 * d);
            mesh.line(a, b, 4.0 * d, halo);
            mesh.line(a, b, 2.0 * d, line);
            for (x, y) in [a, b] {
                mesh.rect(x - 3.0 * d, y - 3.0 * d, 6.0 * d, 6.0 * d, halo);
                mesh.rect(x - 2.0 * d, y - 2.0 * d, 4.0 * d, 4.0 * d, line);
            }
        }
    } else if let Some(fraction) = compare {
        // 直接使用绘制左右图像的 scissor 边界，杜绝分线和图像分割点的舍入差异。
        if let Some([_, right]) = viewport.compare_scissors(fraction) {
            let x = right.0 as f32;
            let cy = hole.y + hole.height / 2.0;
            mesh.rect(x - 2.0 * d, hole.y, 4.0 * d, hole.height, halo);
            mesh.rect(x - d, hole.y, 2.0 * d, hole.height, line);
            mesh.rect(x - 10.0 * d, cy - 14.0 * d, 20.0 * d, 28.0 * d, halo);
            mesh.rect(x - 8.0 * d, cy - 12.0 * d, 16.0 * d, 24.0 * d, line);
            for dy in [-4.0, 0.0, 4.0] {
                mesh.rect(x - d, cy + (dy - 1.0) * d, 2.0 * d, 2.0 * d, halo);
            }
        }
    }
    mesh
}

/// 小型顶点缓冲反复使用；设备恢复时随其它 GPU 资源一起重建。
pub(super) struct OverlayRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    count: u32,
}

impl OverlayRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("editor-overlay"),
            source: wgpu::ShaderSource::Wgsl(include_str!("overlay.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("editor-overlay"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("editor-overlay"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor-overlay-vertices"),
            size: 1024 * 24,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            buffer,
            count: 0,
        }
    }

    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        viewport: &Viewport,
        tool: Option<ToolOverlay>,
        compare: Option<f32>,
        palette: Option<OverlayPalette>,
    ) {
        self.count = 0;
        let Some(palette) = palette else {
            return;
        };
        let (width, height) = viewport.viewport_size;
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let mesh = mesh(viewport, tool, compare, palette);
        assert!(mesh.vertices.len() <= 1024);
        let mut bytes = Vec::with_capacity(mesh.vertices.len() * 24);
        for [x, y, r, g, b, a] in &mesh.vertices {
            for value in [
                2.0 * x / width - 1.0,
                1.0 - 2.0 * y / height,
                *r,
                *g,
                *b,
                *a,
            ] {
                bytes.extend_from_slice(&value.to_ne_bytes());
            }
        }
        self.count = mesh.vertices.len() as u32;
        if !bytes.is_empty() {
            queue.write_buffer(&self.buffer, 0, &bytes);
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.count, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> OverlayPalette {
        OverlayPalette {
            line: Srgb8 {
                r: 255,
                g: 255,
                b: 255,
            },
            halo: Srgb8 { r: 0, g: 0, b: 0 },
            crop: Srgb8 { r: 0, g: 255, b: 0 },
            rotate: Srgb8 { r: 255, g: 0, b: 0 },
        }
    }
    fn viewport() -> Viewport {
        let mut v = Viewport::default();
        v.viewport_size = (1200.0, 900.0);
        v.clip_rect = Some(ClipRect {
            x: 100.0,
            y: 80.0,
            width: 900.0,
            height: 700.0,
        });
        v.image_size = (600, 400);
        v.refit();
        v
    }
    #[test]
    fn no_tool_no_vertices_and_zero_hole_is_safe() {
        let mut v = viewport();
        assert!(mesh(&v, None, None, palette()).vertices.is_empty());
        v.clip_rect = Some(ClipRect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        });
        assert!(mesh(&v, None, Some(0.5), palette()).vertices.is_empty());
    }
    #[test]
    fn compare_line_uses_exact_scissor_edge_for_every_dpr_and_endpoint() {
        for dpr in [1.0, 1.375, 2.0] {
            let mut v = viewport();
            v.dpr = dpr;
            for fraction in [0.0, 0.3333, 0.5, 1.0] {
                let m = mesh(&v, None, Some(fraction), palette());
                let edge = v.compare_scissors(fraction).unwrap()[1].0 as f32;
                assert_eq!(m.vertices[0][0], edge - 2.0 * dpr);
                assert_eq!(m.vertices[6][0], edge - dpr);
            }
        }
    }
    #[test]
    fn crop_rotation_and_straighten_fit_buffer_and_share_viewport() {
        for dpr in [1.0, 1.375, 2.0] {
            let mut v = viewport();
            v.dpr = dpr;
            v.rotation = 27.0;
            v.refit_rotated();
            let crop = crate::develop::geometry::largest_centered_rect((600, 400), 27.0, 1.5);
            let tool = ToolOverlay {
                crop,
                source: (600, 400),
                handles: true,
                straighten: None,
            };
            let m = mesh(&v, Some(tool), None, palette());
            assert!(m.vertices.len() < 1024);
            assert!(m.vertices.iter().flatten().all(|v| v.is_finite()));
            let css = v.frame_rect_css(crop, (600, 400)).unwrap();
            assert_eq!(m.vertices[2][1], v.effective_rect().y + css.y * dpr);
            let with_line = mesh(
                &v,
                Some(ToolOverlay {
                    handles: false,
                    straighten: Some([(180.0, 120.0), (360.0, 240.0)]),
                    ..tool
                }),
                None,
                palette(),
            );
            // 旋转无把手，拉线有深浅两层 + 两端点，移动立即改变顶点。
            let without_line = mesh(
                &v,
                Some(ToolOverlay {
                    handles: false,
                    ..tool
                }),
                None,
                palette(),
            );
            assert_eq!(with_line.vertices.len(), without_line.vertices.len() + 36);
            assert!(with_line.vertices.len() < 1024);
        }
    }
}
