// Four-scale undecimated B3-spline (a trous) wavelets, in linear Y / (B-Y) / (R-Y).
// Each dispatch has identical packed row stride: width pixels, never an aligned texture row.
struct Params { width: u32, height: u32, step: u32, scale: f32, luma: f32, chroma: f32, pad: vec2f }
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read_write> pixels: array<vec2u>;
@group(0) @binding(2) var<storage, read_write> low: array<vec4f>;
@group(0) @binding(3) var<storage, read_write> work: array<vec4f>;
@group(0) @binding(4) var<storage, read_write> next: array<vec4f>;
@group(0) @binding(5) var<storage, read_write> removed: array<vec4f>;
fn valid(q: vec3u) -> bool { return q.x < p.width && q.y < p.height; }
fn at(q: vec2i) -> u32 {
    let v = clamp(q, vec2i(0), vec2i(i32(p.width)-1, i32(p.height)-1));
    return u32(v.y)*p.width + u32(v.x);
}
fn rgb(i: u32) -> vec3f {
    let packed = pixels[i];
    return vec3f(f32(packed.x & 65535u), f32(packed.x >> 16u), f32(packed.y & 65535u)) / 65535.0;
}
fn planes(c: vec3f) -> vec4f {
    let y = dot(c, vec3f(0.2126, 0.7152, 0.0722));
    return vec4f(y, c.b-y, c.r-y, 0.0);
}
@compute @workgroup_size(16, 16) fn initialize(@builtin(global_invocation_id) q: vec3u) {
    if !valid(q) { return; }
    let i = q.y*p.width+q.x;
    low[i] = planes(rgb(i));
    removed[i] = vec4f(0.0);
}
@compute @workgroup_size(16, 16) fn horizontal(@builtin(global_invocation_id) q: vec3u) {
    if !valid(q) { return; }
    let v = vec2i(q.xy); let d = vec2i(i32(p.step), 0);
    work[q.y*p.width+q.x] = (low[at(v-2*d)] + 4.0*low[at(v-d)] + 6.0*low[at(v)] + 4.0*low[at(v+d)] + low[at(v+2*d)]) / 16.0;
}
@compute @workgroup_size(16, 16) fn vertical(@builtin(global_invocation_id) q: vec3u) {
    if !valid(q) { return; }
    let v = vec2i(q.xy); let d = vec2i(0, i32(p.step));
    next[q.y*p.width+q.x] = (work[at(v-2*d)] + 4.0*work[at(v-d)] + 6.0*work[at(v)] + 4.0*work[at(v+d)] + work[at(v+2*d)]) / 16.0;
}
// Firm threshold: suppress small noise; retain large edges unchanged, no soft-threshold bias.
fn discarded(detail: f32, threshold: f32) -> f32 {
    return sign(detail) * min(abs(detail), max(0.0, 2.0*threshold-abs(detail)));
}
@compute @workgroup_size(16, 16) fn shrink(@builtin(global_invocation_id) q: vec3u) {
    if !valid(q) { return; }
    let i = q.y*p.width+q.x;
    let d = low[i]-next[i];
    let t = vec3f(p.luma*0.035, p.chroma*0.05, p.chroma*0.05)*p.scale;
    removed[i] += vec4f(discarded(d.x,t.x), discarded(d.y,t.y), discarded(d.z,t.z), 0.0);
}
@compute @workgroup_size(16, 16) fn finish(@builtin(global_invocation_id) q: vec3u) {
    if !valid(q) { return; }
    let i = q.y*p.width+q.x;
    let v = planes(rgb(i))-removed[i];
    let r = v.x+v.z; let b = v.x+v.y;
    let g = (v.x-0.2126*r-0.0722*b)/0.7152;
    let c = vec3u(round(clamp(vec3f(r,g,b), vec3f(0.0), vec3f(1.0))*65535.0));
    pixels[i] = vec2u(c.r | (c.g << 16u), c.b);
}
