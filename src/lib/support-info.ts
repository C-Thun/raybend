import type { BuildInfo } from "./build-info.ts";
/** 只构造明确列出的字段，禁止把 app 状态、库路径或整个 navigator 序列化。 */
export function diagnosticSummary(build:BuildInfo,userAgent:string):string {
  const webview=/(?:Edg|Chrome)\/([\d.]+)/.exec(userAgent)?.[1]??"unknown";
  return JSON.stringify({schema:1,product:"RayBend",version:build.version,channel:build.channel,builtAt:build.builtAt,commit:build.gitHash??null,dirty:build.dirty===true,webview},null,2);
}
