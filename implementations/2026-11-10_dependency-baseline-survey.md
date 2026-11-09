# 地基（依赖 / 工具链）升级调研

> 完成时间：2026-11-10 02:48:07
> 性质：只读调研。**没有改动任何依赖、清单或锁文件**；唯一「真跑」的是 TypeScript 7 的对照 typecheck（用 `pnpm dlx` 在仓库外跑的临时副本）。

## 0. 一句话结论

**真正值得马上做的只有三件：TypeScript 7（已实测通过）、Rust 工具链 1.99.1、Tauri 2.12（带安全修复）。**
**Solid 2.0 现在不能升**——不是「难」，是「还没到」：上游仍是 rc.21，我们的组件原语层（Ark UI）没有发布声明支持 Solid 2 的稳定版，
而迁移面是 193 个 `createEffect` + 117 个 `batch` + 「写信号后立刻读」的调度语义整体变化。

---

## 1. 现状基线（本机实测）

| 层 | 现状 | 上游最新 | 差 |
| --- | --- | --- | --- |
| Node | **v26.5.0** | 26.13.0（26 线） | 同大版本落后 8 个小版 |
| pnpm | **12.9.1** | 12.18.0 | 同大版本落后 9 个小版 |
| Rust | **1.98.1**（`rust-toolchain.toml` 锁定） | 1.99.1（2026-10-15） | 落后 1 个大版 |
| rustup | 1.29.1 | 1.29.3 | patch |
| SolidJS | **1.9.15**（清单 ^1.9.3） | 1.9.18（稳定）/ 2.0.0-rc.21（next） | patch / 跨大版 |
| @solidjs/router | **1.0.0** | 1.0.1 / 2.0.0-next.44 | patch |
| Ark UI | **5.39.2** | 5.40.0 / 6.0.0-next.0 | minor |
| Tabler 图标 | **3.46.0** | 3.51.0 | minor |
| Vite | **8.3.0** | 8.3.7（**无 9**） | patch |
| Tailwind | **4.3.3** | 4.3.5（**无 5**） | patch |
| TypeScript | **6.0.3** | **7.0.2**（7.1 在 beta） | 跨大版 |
| Tauri（crate） | **2.11.5** | 2.12.5 / 3.0.0-alpha.6 | minor / 跨大版 |
| wgpu | **30.0.1** | 30.0.1 | 已最新 |
| image | **0.25.10** | 0.25.10 | 已最新 |
| rawler | **0.8.0** | 0.8.0 | 已最新 |
| lensfun | **0.7.0** | 0.7.0 | 已最新 |
| rusqlite | **0.40.2** | 0.41.1 | 0.x 破坏 |
| dav1d（Windows 静态库） | **1.5.0** | 1.5.4 | patch（人工装） |

规模参照：前端 473 个文件 / 89,638 行（116 `.tsx` + 357 `.ts`，155 个文件 import `solid-js`）；
Rust 260 个文件 / 134,873 行。

---

## 2. 核心问题：Solid 2.0 能不能升？

### 2.1 结论：**不能，现在**

判据不是「怕麻烦」，是三条硬事实：

1. **上游还没 GA**：`solid-js` 的 `next` 是 `2.0.0-rc.21`（2026-11-04）。RC 已连发 21 版，节奏仍是一两周一个
   —— 说明还在改 API，不在「稳定」状态。`@solidjs/web`（2.0 后 JSX 运行时搬去的包）同样是 `2.0.0-rc.21`。
2. **组件原语层没有稳定支持**：我们所有可交互控件都建立在 Ark UI 上（`@ark-ui/solid`），
   `6.0.0-next.0`（2026-10-05）的 **peer 仍是 `solid-js: >=1.6.0`**，没有声明 Solid 2 兼容；
   维护者在 discussion #3852 里说「Solid 2 支持随下一个大版本一起发」。也就是说：**Ark 6 稳定版 = 我们的解锁条件**。
3. **迁移面是「语义重写」而不是「改名字」**：Solid 2 的 `createEffect` 签名变成
   `createEffect(compute, effectFn)` —— **两个参数都必填**（已在 `@solidjs/signals@2.0.0-rc.21` 的 `signals.d.ts` 里核实，
   没有单参重载）。我们仓里 193 处单回调 `createEffect` 全部要按语义拆开，每一处都要判断
   「这是依赖声明还是副作用」——这是最容易把行为改错的一类改动。

### 2.2 迁移面清单（仓内实测计数）

| Solid 1.x 用法 | 仓内命中 | Solid 2 的处置 | 难度 |
| --- | --- | --- | --- |
| `createEffect(fn)` | **193** | 拆成 `compute` + `apply`（两参数必填） | ★★★ 逐处判断语义 |
| `batch(...)` | **117** | 移除（写默认微批，必要时 `flush()`） | ★★ 删除即可，但要查「批内读写」依赖 |
| `onMount(...)` | **83** | `onSettled(...)`，且可直接 return 清理函数 | ★ 机械替换 |
| `untrack(...)` | 72 | 保留（两版都有） | — |
| `splitProps(props, [...])` | **44** | `omit(props, ...)`；**注意它是「返回剩余」，不是「返回 [local, rest] 元组」**，解构写法要重写 | ★★ |
| `type JSX` from `solid-js` | 54 处 | 改从 `@solidjs/web` 取（`solid-js` 不再拥有 JSX 类型） | ★ 机械 |
| `classList={{...}}` | **25** | `class={["x", {y: cond()}]}`（数组/对象形态） | ★★ 每处改写法 |
| `createRoot(...)` | **20** | 嵌套 root 的**归属变了**（2.0 里被父 owner 拥有，随父销毁）；要逐处确认是否依赖旧「独立存活」语义 | ★★★ |
| `from "solid-js/web"` | 11 个文件 | → `@solidjs/web` | ★ 机械 |
| `on([...], fn)` 辅助器 | 7 | 删除（split effect 就是显式依赖） | ★★ |
| `createComputed(...)` | 6 | `createMemo` / split effect / 函数式 `createSignal` 三选一 | ★★ |
| `<Ctx.Provider value>` | 5 | `<Ctx value>`（context 自己就是 Provider） | ★ |
| `<Index each>` | 5 处元素 | `<For keyed={false}>`；**注意 LUT 面板/对比视图那两处是「按位置复用 DOM」的刻意选择**，迁移必须保住这个语义 | ★★ |
| `<Suspense>` | 1 | `<Loading>` | ★ |
| `<Dynamic component>` | 2 | 2.0 里仍保留（已 deprecated），可先不动 | ★ |
| **信号写入变成微任务批** | 全局 | `setX()` 之后**立刻读仍是旧值**；强制同步要 `flush()` | ★★★ 影响所有命令式流程 |

最后一条最危险：我们是桌面应用，大量流程是「写状态 → 立刻读回来做决定」（viewport、撤销栈、编辑确认、XMP 写出）。
Solid 2 把写入改成默认微批之后，这类代码会静默变行为，**不是编译错误**。官网（Solid 2 项目）在
`website/AGENTS.md` §8 已经把这条写成纪律（测试里必须 `await flush()`），说明这不是理论风险。

### 2.3 收益侧：对我们很薄

Solid 2 的头牌是「async 进反应式图」（`createResource` 被删、异步 memo、`Loading` 边界）。
**我们仓里 `createResource` 用了 0 次**，`SuspenseList` 0 次、`startTransition` 0 次 —— 因为数据全走
Rust IPC + 自己的 store，不靠 Suspense 拉数据。也就是说：**升级换来的是「没有的功能」，付出的是全量反应式层重写。**

### 2.4 触发条件（建议改写 `memory/FUTURE.md` A1）

同时满足才开工：
1. `solid-js` 发布 **2.0.0 正式版**（不是 rc）；
2. `@ark-ui/solid` **稳定版**且 peer 明确写 `solid-js: ^2`；
3. `@solidjs/router` 发布 2.0 **正式版**（现在 `2.0.0-next.44`）——我们只在 `src/index.tsx` 一处用它，成本可忽略，但没正式版就不该锁进去。

**唯一可以先做的准备**：官网（`website/`）已经在 Solid 2 上（`2.0.0-rc.6`），把它跟到 `rc.21` 保持「金丝雀」有效
（`pnpm up solid-js@next @solidjs/web@next`，独立 `node_modules`，不影响本体）。真正的迁移应在单独里程碑里做，
并直接用 `@solidjs/diagnostics`（官网已用通）抓 `REACTIVE_WRITE_IN_OWNED_SCOPE` / `STRICT_READ_UNTRACKED` 这类问题。

---

## 3. 其它各层逐项判断

### 3.1 TypeScript 6.0.3 → 7.0.2 ★ 唯一「已实测通过」的大版本

用 `pnpm dlx typescript@7.0.2` 的临时副本对**本仓当前代码**跑了一遍（未改 `package.json`）：

| | TS 6.0.3（现有） | TS 7.0.2（原生 Go 编译器） |
| --- | --- | --- |
| `pnpm typecheck`（`tsc --noEmit`） | 6.03 s，0 error | **0.74 s**，0 error |
| 统计 | — | Files 7494 / Lines 310,355 / Check 0.481 s / Mem 462 MB |
| `tsconfig.node.json` 子项目 | 通过 | 通过 |
| 配置改动 | — | **零**（`target: ES2020`、`moduleResolution: bundler`、`isolatedModules` 全部仍合法） |

为什么能直接上：TS 7 把 6 里 deprecated 的东西变成硬错误（`target: es5`、`moduleResolution: node10`、`baseUrl`、
`downlevelIteration` 等），我们一条都没踩。**真正的风险是「没有 compiler API」**：TS 7.0 不提供库式 API，
凡是 `import "typescript"` 的工具（类型感知 lint、ts-jest 等）都跑不了。已核实：本仓 `scripts/` 与 `src/`
**没有任何** `from "typescript"`；质量门里只有 `tsc`、`vite`、`node --test`、自写 lint 脚本 —— 全都不需要它。
（`knip.json` 在仓里但没有对应 npm script，也不参与门禁。）

**判断：升。** 这是本轮性价比最高的一项（约 8 倍提速，且零配置成本）。

### 3.2 Rust 工具链 1.98.1 → 1.99.1

- 1.99.0（2026-10-01）+ 1.99.1（2026-10-15，修 x86_64 FFI bool 返回误编译、UEFI `wcslen`）。
  语言层只有 **allow-by-default** 的新 lint，没有破坏性变更。
- 实证收益：`cargo update --dry-run` 明确写着「Locking 161 packages to **latest Rust 1.98** compatible versions」，
  并有 **15 个包因为 MSRV 卡在旧版**。工具链一动，这批就能一起收。
- 注意事项：`rust-toolchain.toml` + `[workspace.package] rust-version` 要**同时**改成 1.99（两处不一致会让
  某些依赖解析回退）；切 toolchain 会让 cargo 的编译单元全部失效 → 一次全量重建（本轮没跑，见 §6）。

### 3.3 Tauri 2.11.5 → 2.12.5 ★ 有安全修复，建议尽快

- **安全**：2.12.0 修了 GHSA-w28w-mhc8-qvjv（IPC channel 队列条目可被别的 webview 读到）。
  我们目前 `src-tauri` **没有用 `ipc::Channel`**（已 grep 确认），所以不是「正在被打」，但这是白送的修复。
- **两个必须同步的 pin**（这是最容易踩的一处）：
  - `tauri 2.12` 依赖 `webview2-com ^0.39` + `windows ^0.62`，而我们 `Cargo.toml` 里钉死
    `webview2-com = "=0.38.2"`、`windows-core = "=0.61.2"`。不改这两个 pin，`cargo update` 会**同时存在两份
    webview2-com**（dry-run 已经显示 `Adding webview2-com v0.39.1`）—— `desktop_behavior.rs` 里
    `ICoreWebView2Settings3` 的 COM 接口会因类型不是同一份而编译失败。**改 tauri 必须同一次改这两个 pin。**
  - `tauri-plugin-updater 2.14` 要 `minisign-verify ^0.3`，我们的清单是 `0.2`（dry-run 同样显示会新增一份 0.3.0）。
    同一次改动里把清单提到 `0.3`，否则又是两份。
- 其它：2.12 移除 Windows 7 支持（我们是 Win10/11，无影响）；`tauri` 迁到 edition 2024、MSRV 1.90。
- JS 侧要一起动：`@tauri-apps/api` 2.11.1 → 2.12.4、`@tauri-apps/cli` 2.11.4 → 2.12.4、
  `@tauri-apps/plugin-dialog` 2.7.3 → 2.8.4。**注意 `scripts/release.mjs` 会校验 Windows 侧
  `cargo-tauri` 与前端 CLI 版本严格相等**，CLI 一动就得让崔总在 Windows 上重装一次
  `cargo install tauri-cli --version 2.12.4 --locked`。

### 3.4 Tauri 3.0：不升（alpha.6，2026-11-06）

仍是 alpha（7 个 alpha，从 2026-09-13 起），已知破坏面：`wry` / `macos-private-api` 从应用依赖里移除、
运行时 crate 化（显式依赖 `tauri-runtime-wry`）、插件初始化脚本重命名、顶层导出重整、Linux 迁 GTK4、
MSRV 1.95、CEF 运行时可选。我们「不引用 `tauri::Wry`、不依赖 feature flag」的纪律（AGENTS.md §6.2）确实让迁移变浅，
但 alpha 阶段连 `tauri migrate` 都还在动。**触发条件不变：beta/rc + 官方插件同步发布。**（`memory/FUTURE.md` A2 已登记，结论不变。）

### 3.5 前端一批 patch / minor（可以做，无风险）

| 包 | 现 | 新 | 说明 |
| --- | --- | --- | --- |
| solid-js | 1.9.15 | 1.9.18 | 1.9 线还在维护（rc 期间连着发了 1.9.16–1.9.18） |
| @solidjs/router | 1.0.0 | 1.0.1 | |
| @ark-ui/solid | 5.39.2 | 5.40.0 | |
| @tabler/icons-solidjs | 3.46.0 | 3.51.0 | 图标是纯增；dev 冷启动会多扫模块（vite 配置里已有注释） |
| vite | 8.3.0 | 8.3.7 | **不存在 Vite 9**（dist-tag 里 `alpha` 还停在 6.0.0-alpha.24） |
| vite-plugin-solid | 2.11.14 | 2.11.15 | |
| tailwindcss / @tailwindcss/vite | 4.3.3 | 4.3.5 | **不存在 Tailwind 5**（`next` 标签指向 4.0.0） |
| @fontsource-variable/* | 5.3.0 | 5.3.1 | 注意：CJK metrics 覆盖插件按「~94 个切片」断言，升级后 `pnpm smoke:ui` 要看一眼 |
| @types/node | 26.5.1 | 26.6.6 | |

### 3.6 Rust 依赖：分三档

#### A. 不改清单、`cargo update` 就能拿到的（161 个包更新，直接相关的高危项）

`tauri 2.11.5→2.12.5`、`tauri-build 2.6.3→2.7.4`、`wry 0.55.1→0.57.1`、`tao 0.35.3→0.37.2`、
`trash 5.2.9→5.3.0`、`kamadak-exif 0.6.1→0.6.2`、`notify 8.2.0→8.2.2`、`flate2 1.1.10→1.1.11`、
`serde_json 1.0.151→1.0.154`、`tokio 1.53.1→1.54.1`、`rustls 0.23.45→0.23.46`、`thiserror 2.0.20→2.0.21`、
`libc 0.2.189→0.2.191`、`unicode-normalization 0.1.25→0.1.26`、`tempfile`/`semver`/`futures-util` 已是末版。

**但别裸跑 `cargo update`**：dry-run 显示它会顺手新增 `webview2-com 0.39.1`、`minisign-verify 0.3.0`、
`zip 9.5.0` 三份**并列副本**（我们的清单卡在旧大版）。COM 那一份必须按 §3.3 同步改 pin，剩下两份要么同步提清单、
要么接受重复。

#### B. 要改清单（0.x / 跨大版），单开一轮 + 跑测试

| crate | 现 → 新 | 影响面（仓内） | 判断 |
| --- | --- | --- | --- |
| rusqlite | 0.40.2 → 0.41.1 | 整个 `store/`；0.41 带 libsqlite3-sys 0.39（SQLite 本体更新）、edition 2024、`zeroblob` 用 u64、**新增 PRAGMA 名校验**（0.41.1 已给 SQLCipher 开豁免） | 值得升，但要专项回归：迁移框架 + FTS5 trigram + 全部 PRAGMA 语句 |
| pollster | 0.4.0 → 1.0.1 | 2 个文件 5 处 `block_on` | 低风险 |
| sha2 | 0.10.9 → 0.11.1 | 哈希封装（profile 指纹 / 文件去重） | 中低；**哈希输出必须逐位不变**，要跑指纹相关单测 |
| roxmltree | 0.20.0 → 0.21.2 | `xmp/packet.rs`、`export/metadata.rs` | 中低；`Document::parse` 未变，0.21 加的是 `ParsingOptions::entity_resolver` |
| base64 | 0.22.1 → 0.23.1 | `updates/download.rs`、`thumbs.rs` | 低 |
| minisign-verify | 0.2.5 → 0.3.0 | `updates/download.rs` | **必须与 tauri-plugin-updater 2.14 一起**（见 §3.3） |
| zip | 4.6.1 → 9.5.0 | 只有 `diagnostics/bundle.rs` 一个文件（`ZipWriter` + `SimpleFileOptions`） | 低风险但跨度 5 个大版，写完 zip 后要解回来验证一遍 |
| ort | 2.0.0-rc.13 → rc.15 | `ai-runtime` 特性；DLL 固定 ORT 1.28.0 / api-27 | 先不动：精确钉死是有意的（`specs/photo-organization-ai-w3.md`），换版本要重验模型包 |
| webview2-com / windows-core | 0.38.2 → 0.39.1 / 0.61.2 → 0.62.x | `desktop_behavior.rs` | **跟 Tauri 2.12 同批** |

#### C. 已是最新，无需动作

`wgpu 30.0.1`、`image 0.25.10`、`rawler 0.8.0`、`lensfun 0.7.0`、
`lcms2 6.2.0`、`webp 0.3.1`、`png 0.18.1`、`half 2.7.1`、`windows-sys 0.61.2`、`raw-window-handle 0.6.2`、
`reqwest 0.13.5`、`crash-handler 0.8.1`、`minidumper 0.11.0`。

### 3.7 dav1d（Windows AVIF 解码静态库）

`scripts/lib/dav1d-win.mjs` 钉 `dav1d-1.5.0`，上游已到 **1.5.4**（1.5.1–1.5.4 都是修 bug）。
这是**人工步骤**（Windows 侧 `C:\rb-deps\`），改了要重跑 `pnpm debug:win` + `check:win` 才看得到效果。

### 3.8 Node / pnpm

- Node v26.5.0 → 26.13.0（同一条 LTS 线内的小版本，`engines` 现在写 `>=22.12.0`，不需要改）。
  网站 CI 单独钉 Node 24（`.github/workflows/website.yml`），与本体无关。
- pnpm 12.9.1 → 12.18.0：`packageManager` 字段**由 pnpm 自己改写**（AGENTS.md 明令不要手工动）。
  升级只需 `npm i -g pnpm@12.18.0`（本机全局 CLI 用 npm 装），之后 `pnpm install` 会更新 lock 的版本标记。

### 3.9 官网（`website/`，独立栈）

它已经在 Solid 2 上：`solid-js` / `@solidjs/web` **2.0.0-rc.6**、`@solidjs/router` 2.0.0-next.21、
`@solidjs/vite-plugin` 3.0.0-next.38、`filesystem-routing` 0.2.1。与最新 `next`（rc.21）差 15 个 RC，
但 `pnpm outdated` **不会**报它（prerelease 不在 `latest` 标签上）——要手动 `pnpm up solid-js@next @solidjs/web@next`。
另外 `lucide-solid` 1.46 与 Solid 2 不兼容（官网已改成自生成图标，`website/AGENTS.md` §4.5 有记录），
**这条经验对本体也成立：Solid 2 迁移时所有只支持 Solid 1 的第三方 UI 库都得先找到替代。**

---

## 4. 建议分期

### P0（可以马上做，风险低，一次性做完）
1. **TypeScript 6.0.3 → 7.0.2**（`typescript: "~7.0"`）→ `pnpm typecheck && pnpm test && pnpm build`。
2. **Rust 1.98.1 → 1.99.1**（`rust-toolchain.toml` + `[workspace.package] rust-version` 两处）。
3. **Tauri 2.11.5 → 2.12.5**：crate + `tauri-build` + 三个插件 + `webview2-com =0.39.1` + `windows-core =0.62`
   + `minisign-verify =0.3` + JS 侧 `@tauri-apps/*` 同步。**这条要崔总配合**：Windows 上重装
   `cargo install tauri-cli --version 2.12.4 --locked`。
4. 前端 patch/minor 一批（§3.5）+ `cargo update`（先按 §3.6A 的坑把清单对齐）。
5. pnpm 12.18.0。

### P1（单独一轮，带回归）
- rusqlite 0.41.1（迁移框架 + FTS5 + PRAGMA 专项回归）
- sha2 0.11 / roxmltree 0.21 / base64 0.23 / pollster 1.0 / zip 9（同一轮「小跨度破坏项」打包）
- dav1d 1.5.4（人工）

### P2（等上游，只做观测）
- **Solid 2.0**：等 GA + Ark 稳定版（§2.4）。此间只把官网跟到 rc.21 保温。
- **Ark UI 6**：跟随 Solid 2 一起；不发稳定版不动。
- **Tauri 3**：等 beta/rc + 插件齐。
- **Vite 9 / Tailwind 5**：目前不存在，不用管。
- **ort rc.15**：等模型包重验的窗口，跟 AI 功能一起动。
- **edition 2027**：Rust 常量在 2027 年才有，届时单开迁移（edition 2024 已就位）。

---

## 5. 坑与纪律提醒（本轮实证）

1. **`webview2-com` / `windows-core` 的 pin 是「与 tauri 版本绑定」的**，不是随手写的版本号。改 tauri 必须同批改。
2. **裸跑 `cargo update` 会制造重复副本**（webview2-com / minisign-verify / zip），dry-run 已经能看到；
   重复副本不会报错，但会在「同一类型两份定义」时以编译错误甚至运行期失败的形式出现。
3. **`cargo update --dry-run` 本身是安全操作**（不改锁文件），以后评估 Rust 侧可以直接用它取证。
4. **构建/发布期间禁改工作树**（release 按构建时文件快照校验），这批升级要避开发版窗口。
5. **TS 7 之后不要引入任何 `import "typescript"` 的工具**（没有 compiler API）。要引入类型感知 lint，
   得等 7.1 的新 API 或继续留在 6.x 的工具链上。

---

## 6. 本轮验证范围（如实）

| 做了什么 | 结果 |
| --- | --- |
| `pnpm outdated`（根 + website） | 见 §3.5 / §3.9 |
| crates.io sparse index 逐个查最新版 | 见 §1 / §3.6 |
| `cargo update --dry-run [--verbose]` | 161 包可升、15 包被 MSRV 卡住、3 处会产生重复副本 |
| `rustup check` | stable 1.99.1 可用 |
| `npm view <pkg> dist-tags/time/peerDependencies` | Solid/Ark/router/vite-plugin 的 RC 与 peer 证据 |
| Solid 2 迁移面静态度量（`rg` 计数） | 见 §2.2 |
| `@solidjs/signals@2.0.0-rc.21` 类型定义 | 确认 `createEffect(compute, effectFn)` 无单参重载 |
| **TS 7.0.2 对现仓 typecheck** | **0 error，0.74s**（TS 6.0.3: 6.03s） |

**没有做**（按纪律留给人 / 下一轮）：
- 没跑 `cargo test --workspace`、没做 Windows 真机构建（本轮没有代码改动，不做无谓的整树编译）。
- 没做 Solid 2 迁移试验（spike）——建议在 Ark 稳定版出来之后单独立项，先拿 `src/dev/KitchenSink.tsx` 那一页做试验田。
- 没做 `cargo audit`（RustSec 全量扫描）——可作为下一轮的安全面补充。

---

## 7. 按需改内存

- `memory/FUTURE.md` A1（Solid 2 迁移）的「现状」已过期（写的 rc.8 / next.24），触发条件建议按 §2.4 细化。
  **本轮未改**——结论归崔总拍板，说了再动。
- `memory/ARCHITECTURE.md` §4 版本基线表（Solid / 路由 / Tauri / wgpu 等）只有 P0 做完之后才该改。
