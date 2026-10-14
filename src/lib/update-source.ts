export type UpdateMode = "off" | "stable" | "beta" | "custom";
export interface UpdateSource {mode:UpdateMode; endpoint:string; publicKey:string}
export const UPDATE_KEY="raybend.updates.v1";
/** 开机静默检查的偏好键：默认开，只有显式存 "off" 才关（损坏/缺失都当开） */
export const AUTO_UPDATE_KEY="raybend.updates.auto.v1";
/** 启动后延迟多久检查（毫秒）：先让首屏与建库把路走完，不抢启动时的带宽与主线程 */
export const AUTO_UPDATE_DELAY_MS=8000;
export const UPDATE_ENDPOINTS={stable:"https://github.com/C-Thun/raybend/releases/latest/download/latest.json",beta:"https://raybend.cthun.com/updates/beta.json"};
export function validUpdateSource(source:UpdateSource):boolean {
  if(!["stable","beta","custom"].includes(source.mode) || !source.publicKey.trim() || source.publicKey.length>8192 || source.endpoint.length>4096)return false;
  try{const url=new URL(source.endpoint);return url.protocol==="https:" && !!url.hostname && !url.username && !url.password && !url.hash;}catch{return false;}
}
export function defaultUpdateSource(publicKey:string,channel:string):UpdateSource {
  /* 测试/开发包默认不自动检查（避免测试环境被拉到正式版）；用户仍可在更新对话框里手动选稳定/预览源。 */
  const mode=publicKey ? (channel==="release"?"stable":channel==="beta"?"beta":"off") : "off";
  return {mode,endpoint:mode==="off"?"":UPDATE_ENDPOINTS[mode],publicKey};
}
export function readUpdateSource(raw:string|null, fallback:UpdateSource):UpdateSource {
  try{const source=JSON.parse(raw??"null");if(source?.mode==="off")return {mode:"off",endpoint:"",publicKey:""};if(source && typeof source.endpoint==="string" && typeof source.publicKey==="string" && validUpdateSource(source))return source;}catch{/* 损坏偏好回到构建配置。 */}return fallback;
}
/** 开机自动检查是否开着（`raw!=="off"`：默认开，只有明确关过才不查） */
export function readAutoCheck(raw:string|null):boolean{return raw!=="off";}
/**
 * 启动静默检查该用哪个源：用户偏好优先，`off` / 缺公钥（无签名验证）返回 `null`。
 * 自动检查与更新对话框读同一份偏好，不会出现「对话框用 beta、后台偷查 stable」的错位。
 */
export function autoCheckSource(raw:string|null, fallback:UpdateSource):UpdateSource|null {
  const source=readUpdateSource(raw,fallback);
  return source.mode==="off"||!source.publicKey.trim()?null:source;
}
