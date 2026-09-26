/** 首启提示属于设备偏好；没有旧版 key，不迁移 catalog 数据。 */
export const WELCOME_KEY = "raybend.welcome.v1";
export function shouldWelcome(native: boolean, repositories: number, acknowledged: string | null): boolean {
  return native && repositories === 0 && acknowledged !== "done";
}
export function welcomeAcknowledged(storage?: Pick<Storage, "getItem">): string | null {
  try { return (storage ?? globalThis.localStorage)?.getItem(WELCOME_KEY) ?? null; } catch { return null; }
}
export function acknowledgeWelcome(storage?: Pick<Storage, "setItem">): void {
  try { (storage ?? globalThis.localStorage)?.setItem(WELCOME_KEY, "done"); } catch { /* 设备禁写不阻断建库。 */ }
}
