//! spike 的着色器（WGSL 属于 Rust 侧，前端不接触 —— `AGENTS.md` §6.1 红线 #3）。
//!
//! 一张图的四边形：顶点位置**直接用图像像素坐标**（0..w, 0..h），
//! 由 [`crate::render::viewport::Viewport::matrix`] 算出的矩阵搬到 NDC。
//! 这样视口变换只有**一处**（Rust 的 `Viewport`），着色器里没有第二套数学 ——
//! 覆盖层要复用同一套时，直接拿同一个矩阵就行。
//!
//! 顶点不用顶点缓冲：4 个角由 `vertex_index` 现场算（三角带顺序 TL/TR/BL/BR）。
//! 少一个缓冲就少一处能写错的地方。

struct Uniforms {
    /// 图像像素 → NDC（列主序，`Viewport::matrix()` 直接喂进来）
    transform: mat4x4<f32>,
    /*
     * `.x` = 整体不透明度，`.yzw` 未用。
     *
     * 刻意写成 `vec4` 而不是 `f32 + vec3`：**uniform 的 vec3 要 16 字节对齐**，
     * 于是那个写法实际占 96 字节（f32 在 64、vec3 被迫挪到 80）——
     * 而 Rust 侧按 80 分配，wgpu 在**绘制时**报「bound with size 80 where the shader expects 96」。
     * 这个错就是离屏冒烟抓到的。用 vec4 则 64 + 16 = 80，两边一眼对得上。
     */
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var image_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    /// 图像内的归一化坐标（0..1）
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    // 三角带：0=左上 1=右上 2=左下 3=右下
    let dims = vec2<f32>(textureDimensions(image, 0));
    let corner = vec2<f32>(f32(index & 1u), f32(index >> 1u));
    let pixel = corner * dims;

    var out: VsOut;
    out.position = uniforms.transform * vec4<f32>(pixel, 0.0, 1.0);
    out.uv = corner;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // 纹理是 sRGB 格式：采样时硬件自动转成线性，写回 surface 时再转回去
    let texel = textureSample(image, image_sampler, in.uv);
    return vec4<f32>(texel.rgb, texel.a) * uniforms.params.x;
}
