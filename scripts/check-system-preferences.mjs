/** 可重复的首启 DOM 冒烟；使用公共 CDP 工装，不声称 Windows/macOS/Linux 真机 E2E。 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { launchChrome, connectCdp, requireServer, sleep } from "./lib/cdp.mjs";

const url = process.argv[2] ?? "http://localhost:1420/";
const port = Number(process.env.CDP_PORT ?? 9529);
// 与 Rust include_str 注入的脚本相同；DOM 冒烟模拟文档初始化，不声称原生窗口验收。
const splashInitialization = readFileSync(new URL("../public/splash-locale.js", import.meta.url), "utf8");
await requireServer(url);
const chrome = launchChrome({ port });
let cdp;
try {
  cdp = await connectCdp(port);
  const evaluate = async (expression) => {
    const result = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
    return result.result.value;
  };
  let scriptId;
  let launchNumber = 0;
  async function boot({ theme, language, saved = {}, preserve = false, unavailable = false }) {
    if (scriptId) await cdp.send("Page.removeScriptToEvaluateOnNewDocument", { identifier: scriptId });
    await cdp.send("Page.navigate", { url: "about:blank" });
    await cdp.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
    const config = JSON.stringify({ language, saved, preserve, unavailable, launch: `startup-smoke-${++launchNumber}` });
    const script = await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: `
      const config = ${config};
      let prior = null;
      try { prior = JSON.parse(localStorage.getItem("raybend.splash-launch.v1")); } catch {}
      if (prior?.launch !== config.launch) {
        if (!config.preserve) localStorage.clear();
        for (const [key, value] of Object.entries(config.saved)) localStorage.setItem(key, value);
      }
      Object.defineProperty(navigator, "language", { configurable: true, get: () => config.language });
      if (config.unavailable) {
        Object.defineProperty(window, "localStorage", { configurable: true, get: () => { throw new Error("Storage disabled"); } });
        const originalMatchMedia = window.matchMedia.bind(window);
        window.matchMedia = (query) => {
          if (query.includes("prefers-color-scheme")) throw new Error("System appearance unavailable");
          return originalMatchMedia(query);
        };
      }
      window.__RAYBEND_LAUNCH_ID__ = config.launch;
      ${splashInitialization}
    ` });
    scriptId = script.identifier;
    await cdp.send("Page.navigate", { url });
    // 冷启动时 Vite 预打包可能超过 20 秒；与现有 UI 冒烟使用同一等待上限。
    const deadline = Date.now() + 60_000;
    while (Date.now() < deadline) {
      const state = await evaluate(`(() => {
        const root = document.getElementById("root");
        if (!root?.children.length) return null;
        let saved = {};
        try { saved = { theme: localStorage.getItem("raybend.theme"), locale: localStorage.getItem("raybend.locale") }; } catch {}
        return { theme: document.documentElement.dataset.theme, locale: document.documentElement.lang, saved };
      })()`);
      if (state) return state;
      await sleep(50);
    }
    throw new Error(`首次启动 60 秒内未渲染：${JSON.stringify({ errors: cdp.consoleErrors, exceptions: cdp.exceptions, page: await evaluate("({url:location.href,theme:document.documentElement.dataset.theme,locale:document.documentElement.lang})") })}`);
  }

  const expect = (actual, theme, locale, persist = true) => {
    assert.equal(actual.theme, theme);
    assert.equal(actual.locale, locale);
    if (persist) assert.deepEqual(actual.saved, { theme, locale });
  };
  async function expectSplash(expected) {
    await cdp.send("Page.navigate", { url: new URL("splash.html", url).href });
    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
      const state = await evaluate(`(() => {
        const img = document.getElementById("splash");
        return img?.naturalWidth > 0 ? { lang: document.documentElement.lang, src: img.getAttribute("src") } : null;
      })()`);
      if (state) {
        assert.equal(state.lang, expected);
        assert.equal(state.src, expected === "zh-CN" ? "splash/splash-cn.webp" : "splash/splash-en.webp");
        return;
      }
      await sleep(50);
    }
    throw new Error("splash 图片未加载");
  }
  expect(await boot({ theme: "light", language: "zh-CN" }), "light", "zh-CN");
  await expectSplash("en-US"); // 主界面已保存中文，这次仍必须是首次英文。
  // 重启时系统已改成另一种：使用首次保存的选择。
  expect(await boot({ theme: "dark", language: "en-US", preserve: true }), "light", "zh-CN");
  await expectSplash("zh-CN");
  expect(await boot({ theme: "dark", language: "ja-JP" }), "dark", "en-US");
  expect(await boot({ theme: "light", language: "zh-Hant-TW" }), "light", "zh-CN");
  expect(await boot({ theme: "light", language: "zh-CN", saved: { "raybend.theme": "dark", "raybend.locale": "en-US" } }), "dark", "en-US");
  await expectSplash("en-US");
  expect(await boot({ theme: "dark", language: "en-US", saved: { "raybend.theme": "light" } }), "light", "en-US");
  expect(await boot({ theme: "light", language: "en-US", saved: { "raybend.locale": "zh-CN" } }), "light", "zh-CN");
  expect(await boot({ theme: "light", language: "zh-CN", saved: { "raybend.theme": "junk", "raybend.locale": "junk" } }), "light", "zh-CN");
  expect(await boot({ theme: "light", language: "", unavailable: true }), "dark", "en-US", false);
  assert.deepEqual(cdp.consoleErrors, [], "启动时不应有 console.error");
  assert.deepEqual(cdp.exceptions, [], "启动时不应有未捕获异常");
  console.log("✓ 9 个首次启动/重启场景及首次英文/下次中文/软件英文的 splash 图片加载通过");
} finally {
  cdp?.close();
  chrome.kill("SIGKILL");
}
