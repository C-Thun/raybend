export type UpdateMode = "off" | "stable" | "beta" | "custom";
export interface UpdateSource {mode:UpdateMode; endpoint:string; publicKey:string}
export const UPDATE_KEY="raybend.updates.v1";
export const UPDATE_ENDPOINTS={stable:"https://github.com/C-Thun/raybend/releases/latest/download/latest.json",beta:"https://raybend.cthun.com/updates/beta.json"};
export function validUpdateSource(source:UpdateSource):boolean {
  if(!["stable","beta","custom"].includes(source.mode) || !source.publicKey.trim() || source.publicKey.length>8192 || source.endpoint.length>4096)return false;
  try{const url=new URL(source.endpoint);return url.protocol==="https:" && !!url.hostname && !url.username && !url.password && !url.hash;}catch{return false;}
}
export function defaultUpdateSource(publicKey:string,channel:string):UpdateSource {
  const mode=publicKey ? (channel==="beta"?"beta":"stable") : "off";
  return {mode,endpoint:mode==="off"?"":UPDATE_ENDPOINTS[mode],publicKey};
}
export function readUpdateSource(raw:string|null, fallback:UpdateSource):UpdateSource {
  try{const source=JSON.parse(raw??"null");if(source?.mode==="off")return {mode:"off",endpoint:"",publicKey:""};if(source && typeof source.endpoint==="string" && typeof source.publicKey==="string" && validUpdateSource(source))return source;}catch{/* 损坏偏好回到构建配置。 */}return fallback;
}
