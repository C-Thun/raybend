/**
 * 开发期陈列室共用的**假照片**（`/dev/kitchen-sink`）。
 *
 * 为什么要有它：真机上要看图/对比得先有照片（要后端），而**交互与布局本身**
 * 是可以在这里实测的 —— 关键只有一条：图片尺寸必须是真的（`naturalWidth/Height`
 * 有值，比例才算得出来），字节可以很小。
 *
 * 两个 demo（`viewer-demo` 单张看图、`compare-demo` 对比）共用这一份，
 * 免得各写一张假图、尺寸各说各话。
 */

/** 一张尺寸真实的 SVG data URL（默认 1600×1000） */
export function fakeImageUrl(width = 1600, height = 1000): string {
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">` +
    // 具名颜色：色值只允许写在 tokens.css（`pnpm lint:colors` 管着）；
    // 这里只是张占位图，像不像品牌色不重要，「尺寸真实」才重要
    `<rect width="${width}" height="${height}" fill="darkslategray"/>` +
    `<circle cx="${width / 2}" cy="${height / 2}" r="${Math.min(width, height) * 0.26}" fill="mediumaquamarine"/>` +
    "</svg>";
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/**
 * 三张假照片：**横、竖、超宽**各一张 —— 对比的虚拟画布（不裁不缩、居中贴进画布）
 * 就是为这种尺寸不一致的情况设计的，fake 数据得能盖住它。
 *
 * `natural` 与真机一样由「元数据」给出（不是等 `img.onLoad`），
 * 这样对比进入第一帧就有尺寸、不会先画一次空白。
 */
export const DEMO_PHOTOS = [
  { id: "demo-1", path: "/demo/1.jpg", fileName: "P1000001.JPG", natural: { width: 1600, height: 1000 } },
  { id: "demo-2", path: "/demo/2.jpg", fileName: "P1000002.JPG", natural: { width: 900, height: 1400 } },
  { id: "demo-3", path: "/demo/3.jpg", fileName: "P1000003.JPG", natural: { width: 2400, height: 700 } },
] as const;

/** 每张照片给一份**可区分的**假图（尺寸与 `natural` 一致） */
export function demoImageFor(path: string): string {
  const photo = DEMO_PHOTOS.find((item) => item.path === path);
  return photo === undefined
    ? fakeImageUrl()
    : fakeImageUrl(photo.natural.width, photo.natural.height);
}
