/** 测真实无 bundle 脚本；不同 VM 模拟不同 WebView，共享存储模拟同源设置。 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { runInNewContext } from "node:vm";
import { LOCALE_STORAGE_KEY } from "./index.ts";

const initialization = readFileSync(new URL("../../public/splash-locale.js", import.meta.url), "utf8");
const html = readFileSync(new URL("../../public/splash.html", import.meta.url), "utf8");
const inline = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
assert.ok(inline);
const snapshotKey = "raybend.splash-launch.v1";
function memory(saved: string | null = null) {
  const data = new Map<string, string>();
  if (saved !== null) data.set(LOCALE_STORAGE_KEY, saved);
  return { data, getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value); } };
}
function page(storage: ReturnType<typeof memory>, launch?: string) {
  const window: Record<string, unknown> = { localStorage: storage, __RAYBEND_LAUNCH_ID__: launch };
  const image: Record<string, unknown> = {};
  const document = { documentElement: { dataset: {} as Record<string, string>, lang: "" }, getElementById: () => image };
  const context = { window, document, localStorage: storage };
  if (launch) runInNewContext(initialization, context);
  runInNewContext(inline!, context);
  return { window, image, document };
}

test("首次 splash 英文；主界面首次保存中文也不改变同一次启动快照，下次才中文", () => {
  const storage = memory();
  page(storage, "first"); // 主文档的初始化先执行。
  storage.setItem(LOCALE_STORAGE_KEY, "zh-CN"); // 主界面检测系统中文并保存。
  assert.equal(page(storage, "first").image.src, "splash/splash-en.webp");
  assert.equal(page(storage, "second").image.src, "splash/splash-cn.webp");
  assert.equal(storage.data.size, 2, "每次覆盖同一派生快照，不积累启动记录");
});

test("已有中文/英文按软件设置；本次手动切语言，下次启动才更换 splash", () => {
  const storage = memory("zh-CN");
  const first = page(storage, "first");
  assert.equal(first.image.alt, "光伴");
  assert.equal(first.document.documentElement.lang, "zh-CN");
  storage.setItem(LOCALE_STORAGE_KEY, "en-US");
  assert.equal(page(storage, "first").document.documentElement.dataset.splash, "cn");
  assert.equal(page(storage, "second").image.src, "splash/splash-en.webp");
});

test("两个 WebView 交错读取时，主界面刚保存的中文不能覆盖已固定的首启英文", () => {
  const storage = memory();
  const interleaved = { ...storage, getItem: (key: string) => {
    if (key === LOCALE_STORAGE_KEY) {
      storage.setItem(snapshotKey, JSON.stringify({ launch: "first", locale: "en-US" }));
      storage.setItem(LOCALE_STORAGE_KEY, "zh-CN");
    }
    return storage.getItem(key);
  } };
  assert.equal(page(interleaved, "first").image.src, "splash/splash-en.webp");
});

test("语言缺失/非法和存储不可用默认英文，坏快照不影响合法语言", () => {
  for (const saved of [null, "", "ZH-CN", "中文", "x".repeat(10_000)]) {
    assert.equal(page(memory(saved), "first").image.src, "splash/splash-en.webp");
  }
  const storage = memory("zh-CN");
  storage.setItem(snapshotKey, "{broken");
  assert.equal(page(storage, "first").image.src, "splash/splash-cn.webp");
  storage.setItem(snapshotKey, JSON.stringify({ launch: "second", locale: "junk" }));
  assert.equal(page(storage, "second").image.src, "splash/splash-cn.webp");
  const hostile = { ...storage, getItem: () => { throw new Error("blocked"); }, setItem: () => { throw new Error("blocked"); } };
  assert.equal(page(hostile, "first").image.src, "splash/splash-en.webp");
});

test("无 Tauri 的静态预览读相同软件设置；不写语言也不读系统语言", () => {
  for (const [saved, expected] of [[null, "en"], ["zh-CN", "cn"], ["en-US", "en"]] as const) {
    const storage = memory(saved);
    const result = page(storage);
    assert.equal(result.document.documentElement.dataset.splash, expected);
    assert.equal(storage.getItem(LOCALE_STORAGE_KEY), saved);
  }
});
