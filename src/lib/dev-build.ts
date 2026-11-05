/**
 * 「这次构建带不带开发期面板」的**唯一判定**。
 *
 * 三类构建要区分（崔总 2026-11-01：`pnpm debug:win` 出来的 exe 里也要有调试入口）：
 *
 * | 构建 | `DEV` | `VITE_DEBUG_PANEL` | 带面板 |
 * | --- | --- | --- | --- |
 * | `pnpm dev`（浏览器调试） | `true` | — | 是 |
 * | `pnpm debug:win`（Windows 调试产物） | `false` | `"1"` | **是** |
 * | `pnpm build` / `pnpm release`（要发出去的东西） | `false` | 未设 | 否 |
 *
 * ❗为什么不能只看 `import.meta.env.DEV`：`debug:win` 跑的是**生产前端构建**
 * （`vite build`）＋ debug Rust 产物，`DEV` 恒为 `false` —— 只看它的话，真机调试那份
 * exe 里永远没有入口（2026-11-01 崔总问「debug 面板入口也没有啊」）。
 *
 * ❗为什么写成 `import.meta.env && import.meta.env.VITE_…`：
 * * `node --test` 里没有 Vite 注入的 `import.meta.env` —— 直接取属性会抛错，
 *   靠 `&&` 的短路挡住（`undefined?.DEV` 与 `undefined && …` 都不会求值右半边）；
 * * 字面量形式 `import.meta.env.VITE_…` 才会被 Vite 在构建期替换成常量，
 *   打包器据此把「生产包里的调试入口」整条摇掉（`?.` 形式换不到常量）。
 *
 * 与 Rust 侧的一致性：调试参数真正生效还要求 `cfg!(debug_assertions)`
 * （`src-tauri/src/editor.rs::mask_debug`）—— `debug:win` 是 debug profile，对得上；
 * release 是 `--release`，两边同时关闭。
 */
export const DEV_BUILD: boolean = import.meta.env?.DEV === true
  || (import.meta.env && import.meta.env.VITE_DEBUG_PANEL === "1") === true;
