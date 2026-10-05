//! spike 的着色器（WGSL 属于 Rust 侧，前端不接触 —— `AGENTS.md` §6.1 红线 #3）。
//!
//! 一张图的四边形：顶点位置**直接用逻辑图像像素坐标**（0..w, 0..h），
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
    /*
     * `.xy` = **逻辑图像尺寸**（原图 / 解码尺寸，图像像素）。
     *
     * 顶点用它铺四边形，**不是** `textureDimensions(image)`：当前纹理可能只是预览档（长边 1920），
     * 逻辑尺寸却是 6000 —— 按纹理尺寸铺，「1:1」就变成「预览图的 1:1」（2026-09-24 修）。
     * 纹理只是当前清晰度，UV 仍是 0..1，拉伸与否由这个尺寸决定。
     */
    image_size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var image_sampler: sampler;
// Bound to each image separately: compare images may use different process versions.
@group(0) @binding(3) var<uniform> image_config: vec4<f32>;

struct DisplayTransform {
    row0: vec4<f32>, row1: vec4<f32>, row2: vec4<f32>, params: vec4<f32>,
};
@group(1) @binding(0) var<uniform> display: DisplayTransform;
@group(1) @binding(1) var display_trc: texture_2d<f32>;
@group(1) @binding(2) var display_clut: texture_3d<f32>;
@group(1) @binding(3) var display_sampler: sampler;

struct IccCurve { head:vec4<f32>, tail:vec4<f32>, };
struct IccNative { row0:vec4<f32>,row1:vec4<f32>,row2:vec4<f32>,curves:array<IccCurve,9>,params:vec4<f32>, };
@group(1) @binding(4) var<uniform> icc_native:IccNative;
@group(1) @binding(5) var icc_native_clut:texture_3d<f32>;

struct ToneConfig {
    gains: vec4<f32>, params: vec4<f32>, low: vec4<f32>, high: vec4<f32>,
};
@group(2) @binding(0) var<uniform> tone: ToneConfig;
@group(2) @binding(1) var tone_table: texture_2d<f32>;

struct ProofConfig {
    forward0: vec4<f32>, forward1: vec4<f32>, forward2: vec4<f32>,
    backward0: vec4<f32>, backward1: vec4<f32>, backward2: vec4<f32>, params: vec4<f32>,
};
@group(3) @binding(0) var<uniform> proof: ProofConfig;

fn signed_encode(value: f32) -> f32 {
    let v = abs(value);
    if v <= 0.0031308 { return value * 12.92; }
    return sign(value) * (1.055 * pow(v, 1.0 / 2.4) - 0.055);
}
fn signed_decode(value: f32) -> f32 {
    let v = abs(value);
    if v <= 0.04045 { return value / 12.92; }
    return sign(value) * pow((v + 0.055) / 1.055, 2.4);
}
fn tone_lookup(position: f32, row: i32, channel: u32) -> f32 {
    let size=vec2<f32>(textureDimensions(tone_table));
    let uv=vec2<f32>((clamp(position,0.0,1.0)*(size.x-1.0)+0.5)/size.x,(f32(row)+0.5)/size.y);
    return textureSampleLevel(tone_table,image_sampler,uv,0.0)[channel];
}
fn tone_channel(value: f32, channel: u32) -> f32 {
    if value >= 0.0 && value <= 1.0 {
        var shaped = 0.5 * sqrt(2.0 * value);
        if value > 0.5 { shaped = 1.0 - 0.5 * sqrt(2.0 * (1.0 - value)); }
        return tone_lookup(shaped, 0, channel);
    }
    let encoded = signed_encode(value);
    if encoded < -4.0 { let result=encoded+tone.low[channel];if tone.params.w>0.5 {return signed_decode(result);}return result; }
    if encoded > 4.0 { let result=encoded+tone.high[channel];if tone.params.w>0.5 {return signed_decode(result);}return result; }
    return tone_lookup((encoded + 4.0) / 8.0, 1, channel);
}
fn decode_encoded(value:f32)->f32 {
    if abs(value)<=0.04045 {return value/12.92;}
    if abs(value)>4.0 {return signed_decode(value);}
    return tone_lookup((value+4.0)/8.0,2,0u);
}
fn adjust_global(working: vec3<f32>) -> vec3<f32> {
    if tone.params.x < 0.5 { return working; }
    let input = from_rec2020(working) * tone.gains.xyz;
    var rgb = vec3<f32>(tone_channel(input.r, 0u), tone_channel(input.g, 1u), tone_channel(input.b, 2u));
    if tone.params.w>0.5 {return to_rec2020(rgb);}
    let weights = vec3<f32>(0.2126, 0.7152, 0.0722);
    if abs(tone.params.y) > 0.000001 {
        let luma = dot(weights, rgb); rgb = vec3<f32>(luma) + (rgb - vec3<f32>(luma)) * max(0.0, 1.0 + tone.params.y);
    }
    if abs(tone.params.z) > 0.000001 {
        let luma = dot(weights, rgb); let high = max(max(rgb.r, rgb.g), rgb.b); let low = min(min(rgb.r, rgb.g), rgb.b);
        var current = 0.0;
        if high > 0.000001 { current = clamp((high - low) / high, 0.0, 1.0); }
        rgb = vec3<f32>(luma) + (rgb - vec3<f32>(luma)) * max(0.0, 1.0 + tone.params.z * (1.0 - current));
    }
    return to_rec2020(vec3<f32>(decode_encoded(rgb.r), decode_encoded(rgb.g), decode_encoded(rgb.b)));
}

fn from_rec2020(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(vec3<f32>(1.660491, -0.5876411, -0.07284986), rgb),
        dot(vec3<f32>(-0.1245505, 1.1328999, -0.00834942), rgb),
        dot(vec3<f32>(-0.01815076, -0.1005789, 1.1187297), rgb));
}

fn to_rec2020(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(vec3<f32>(0.627404, 0.329282, 0.0433136), rgb),
        dot(vec3<f32>(0.069097, 0.91954, 0.0113612), rgb),
        dot(vec3<f32>(0.0163916, 0.0880132, 0.895595), rgb));
}

fn trc_channel(value: f32, channel: u32) -> f32 {
    let size=f32(textureDimensions(display_trc).x);
    let position=(sqrt(clamp(value,0.0,1.0))*(size-1.0)+0.5)/size;
    return textureSampleLevel(display_trc,display_sampler,vec2<f32>(position,0.5),0.0)[channel];
}

fn lab_f(v:f32) -> f32 {
    if v > 216.0/24389.0 { return pow(v,1.0/3.0); }
    return (24389.0/27.0*v+16.0)/116.0;
}
fn clut_color(xyz:vec3<f32>) -> vec3<f32> {
    var address = sqrt(clamp(xyz/(65535.0/32768.0),vec3<f32>(0.0),vec3<f32>(1.0)));
    if display.params.x > 2.5 {
        let f = vec3<f32>(lab_f(xyz.x/0.9642),lab_f(xyz.y),lab_f(xyz.z/0.8249));
        address = clamp(vec3<f32>((116.0*f.y-16.0)/100.0,(500.0*(f.x-f.y)+128.0)/255.0,(200.0*(f.y-f.z)+128.0)/255.0)/display.row0.w,vec3<f32>(0.0),vec3<f32>(1.0));
    }
    let size=f32(textureDimensions(display_clut).x);
    let code=textureSampleLevel(display_clut,display_sampler,(address*(size-1.0)+0.5)/size,0.0).rgb;
    return vec3<f32>(signed_decode(code.r),signed_decode(code.g),signed_decode(code.b));
}

fn icc_curve(index:u32,x:f32)->f32 {
    let p=icc_native.curves[index];let kind=p.head.x;let g=p.head.y;let a=p.head.z;let b=p.head.w;
    let c=p.tail.x;let d=p.tail.y;let e=p.tail.z;let f=p.tail.w;
    if kind<0.5 {if g==1.0 {return x;}return pow(max(x,0.0),g);}
    if kind<1.5 {if abs(a)<0.000000001 || x< -b/a {return 0.0;}return pow(max(a*x+b,0.0),g);}
    if kind<2.5 {if abs(a)<0.000000001 {return 0.0;}if x<max(-b/a,0.0) {return c;}if a*x+b<=0.0 {return 0.0;}return pow(a*x+b,g)+c;}
    if x>=d {let v=pow(max(a*x+b,0.0),g);if kind<3.5 {return v;}return v+e;}
    if kind<3.5 {return c*x;}return c*x+f;
}
fn native_clut(rgb:vec3<f32>)->vec3<f32> {
    let dims=vec3<i32>(textureDimensions(icc_native_clut));
    let position=clamp(rgb,vec3<f32>(0.0),vec3<f32>(1.0))*vec3<f32>(dims-1);
    let lo=vec3<i32>(floor(position));let hi=min(lo+1,dims-1);let f=position-vec3<f32>(lo);
    if icc_native.params.y>0.5 {
        var result=vec3<f32>(0.0);
        for(var z=0;z<2;z++) {for(var y=0;y<2;y++) {for(var x=0;x<2;x++) {
            let flag=vec3<i32>(x,y,z)>vec3<i32>(0);let at=select(lo,hi,flag);let w=select(vec3<f32>(1.0)-f,f,flag);
            result+=textureLoad(icc_native_clut,at,0).rgb*w.x*w.y*w.z;
        }}}
        return result;
    }
    var axes=vec3<u32>(0u,1u,2u);
    if f[axes.x]<f[axes.y] {let old=axes.x;axes.x=axes.y;axes.y=old;}
    if f[axes.y]<f[axes.z] {let old=axes.y;axes.y=axes.z;axes.z=old;}
    if f[axes.x]<f[axes.y] {let old=axes.x;axes.x=axes.y;axes.y=old;}
    // FXC cannot write a dynamically indexed vector component. The masks also
    // express the tetrahedron vertices directly without a mutable array lvalue.
    let components=vec3<u32>(0u,1u,2u);
    let one=select(lo,hi,components==vec3<u32>(axes.x));
    let two=select(one,hi,components==vec3<u32>(axes.y));
    return textureLoad(icc_native_clut,lo,0).rgb*(1.0-f[axes.x])+textureLoad(icc_native_clut,one,0).rgb*(f[axes.x]-f[axes.y])+textureLoad(icc_native_clut,two,0).rgb*(f[axes.y]-f[axes.z])+textureLoad(icc_native_clut,hi,0).rgb*f[axes.z];
}
fn native_icc(xyz:vec3<f32>)->vec3<f32> {
    var rgb=xyz/(65535.0/32768.0);
    if icc_native.params.y>0.5 {
        let f=vec3<f32>(lab_f(xyz.x/0.9642),lab_f(xyz.y),lab_f(xyz.z/0.8249));
        rgb=vec3<f32>((116.0*f.y-16.0)/100.0,(500.0*(f.x-f.y)+128.0)/255.0,(200.0*(f.y-f.z)+128.0)/255.0);
    }
    rgb=vec3<f32>(icc_curve(0u,rgb.x),icc_curve(1u,rgb.y),icc_curve(2u,rgb.z));
    rgb=vec3<f32>(dot(icc_native.row0.xyz,rgb)+icc_native.row0.w,dot(icc_native.row1.xyz,rgb)+icc_native.row1.w,dot(icc_native.row2.xyz,rgb)+icc_native.row2.w);
    rgb=vec3<f32>(icc_curve(3u,rgb.x),icc_curve(4u,rgb.y),icc_curve(5u,rgb.z));
    if icc_native.params.x>0.5 {rgb=native_clut(rgb);}
    rgb=clamp(vec3<f32>(icc_curve(6u,rgb.x),icc_curve(7u,rgb.y),icc_curve(8u,rgb.z)),vec3<f32>(0.0),vec3<f32>(1.0));
    return vec3<f32>(signed_decode(rgb.x),signed_decode(rgb.y),signed_decode(rgb.z));
}

fn screen_color(working: vec3<f32>) -> vec3<f32> {
    var shown = working;
    if proof.params.x > 0.5 {
        let device = vec3<f32>(dot(proof.forward0.xyz,working),dot(proof.forward1.xyz,working),dot(proof.forward2.xyz,working));
        let clipped = clamp(device,vec3<f32>(0.0),vec3<f32>(1.0));
        shown = vec3<f32>(dot(proof.backward0.xyz,clipped),dot(proof.backward1.xyz,clipped),dot(proof.backward2.xyz,clipped));
        if proof.params.y > 0.5 && (any(device < vec3<f32>(-0.00001)) || any(device > vec3<f32>(1.00001))) { shown = to_rec2020(vec3<f32>(1.0,0.0,1.0)); }
    }
    if display.params.y > 0.5 {
        let rgb=from_rec2020(shown);let luma=dot(rgb,vec3<f32>(0.2126,0.7152,0.0722));
        if luma<=0.0 {return vec3<f32>(0.0);}
        return rgb/max(luma,1.0)*display.params.z;
    }
    if display.params.x < 0.5 { return clamp(from_rec2020(shown), vec3<f32>(0.0), vec3<f32>(1.0)); }
    let linear = vec3<f32>(dot(display.row0.xyz, shown), dot(display.row1.xyz, shown), dot(display.row2.xyz, shown));
    if display.params.x>3.5 {return native_icc(linear);}
    if display.params.x>1.5 {return clut_color(linear);}
    if display.params.w>0.5 {return clamp(linear,vec3<f32>(0.0),vec3<f32>(1.0));}
    return vec3<f32>(trc_channel(linear.r, 0u), trc_channel(linear.g, 1u), trc_channel(linear.b, 2u));
}

struct VsOut {
    @builtin(position) position: vec4<f32>,
    /// 图像内的归一化坐标（0..1）
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    // 三角带：0=左上 1=右上 2=左下 3=右下
    // 顶点位置是**逻辑图像像素坐标**（0..逻辑尺寸），由矩阵搬到 NDC
    let dims = uniforms.image_size.xy;
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
    if image_config.x > 0.5 {
        // V2 texture: linear Rec.2020 D65 → standard SDR sRGB presentation.
        // This clipping belongs to the screen boundary, never the working image.
        var working = texel.rgb;
        if image_config.y > 0.5 { working = adjust_global(working); }
        return vec4<f32>(screen_color(working), texel.a) * uniforms.params.x;
    }
    if display.params.x > 0.5 || display.params.y > 0.5 || proof.params.x > 0.5 { return vec4<f32>(screen_color(to_rec2020(texel.rgb)), texel.a) * uniforms.params.x; }
    return vec4<f32>(texel.rgb, texel.a) * uniforms.params.x;
}
