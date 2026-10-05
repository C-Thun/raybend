/** Build capability plus native confirmation; no model detection in the frontend. */
export function aiBuildEnabled(): boolean {
  // pi-lens-ignore: no-typeof-undefined, — Vite 注入，Node 测试中不存在
  return typeof __RAYBEND_PHOTO_AI__ !== "undefined" && __RAYBEND_PHOTO_AI__ === true;
}
export function aiCapability(buildEnabled: boolean, nativeCompiled: unknown): boolean {
  return buildEnabled && nativeCompiled === true;
}
declare global { var __RAYBEND_PHOTO_AI__: boolean; }
