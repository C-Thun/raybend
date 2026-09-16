# 品牌资产入场 + 启动闪屏

完成时间：2026-09-17 01:31:26 CST

计划：`plans/branding-and-splash.md`（人类已批，含「图片自己做的、图标就 550 这尺寸别升 1024」这条反馈）。

---

## 1. 本次改动的范围

| # | 改动 | 状态 |
| --- | --- | --- |
| ① | 品牌素材入场（logo / logo-small / 闪屏中英两版），**不转码**、只拷贝 | ✅ |
| ② | 打包图标由 `pnpm tauri icon` 从 `logo.png` 重生成（19 个文件） | ✅ |
| ③ | 标题栏与「关于」的占位图标（Tabler `aperture`）换成真 logo | ✅ |
| ④ | **启动闪屏**：新窗口 + 静态页 + 时序（`ui_ready` / 3 秒兜底 / dev 不弹） | ✅ |
| ⑤ | 前端 `uiReady()` 接线（首屏 + 字体就绪后通知） | ✅ |
| ⑥ | 冒烟：闪屏页 6 条程序化断言（含「无任何可交互元素」） | ✅ |
| ⑦ | `check:win` 补上闪屏资源核对（原先是「假绿」缺口） | ✅ |
| ⑧ | `ARCHITECTURE.md` 新增**素材层 `src/assets/`**（分层检查器同步） | ✅ |
| ⑨ | pi-lens 分流收尾：4 条真·琐碎修掉 + 2 条压制注释 | ✅ |
| ⑩ | Windows 产物重建 + 一致性核对 | ✅ |

---

## 2. 涉及文件

#### 新增

- `src/assets/branding/logo.png`、`logo-small.png`（源：`~/repos/c-thun/`，sha256 逐字节核对）
- `public/splash/splash-cn.webp`、`splash-en.webp`
- `public/splash.html`（零依赖、零打包的静态页）
- `implementations/2026-09-17_branding-and-splash.md`（本文件）

#### 改动

- `src-tauri/tauri.conf.json`：主窗口 `"visible": false`；新增 `splash` 窗口
- `src-tauri/src/lib.rs`：`SPLASH_WINDOW_LABEL`、`SPLASH_TIMEOUT`、`reveal_main()`、`ui_ready` 命令、`setup()` 的 dev/prod 分支
- `src/shell/TitleBar.tsx`、`src/shell/AboutDialog.tsx`：`<img>` 换 `IconAperture`
- `src/api/window.ts`（`uiReady()`）、`src/App.tsx`（`onMount` 竞争后 rAF）
- `src/api/window.test.ts`（+2 条）、`scripts/ui-smoke.mjs`（+闪屏段）
- `scripts/check-architecture.mjs`、`scripts/check-win-artifact.mjs`
- `src-tauri/icons/*`（19 个，由 `pnpm tauri icon` 生成）
- 文档：`ARCHITECTURE.md`、`DESIGN.md`、`design/main.md`（§2.1 + 新 §2.4）、`THIRD-PARTY-NOTICES.md`（§1b）、`PLAN.md`（人类验收清单 3e/3f）
- pi-lens 收尾：`src/i18n/index.ts`、`src/i18n/en-US.ts`、`src/shell/TitleBar.tsx`、`src/shell/AboutDialog.tsx`（导入补 `.ts`）、`src/api/import.ts`（去掉中转变量）、`src/components/ui/tokens.ts`、`src/lib/build-info.ts`（压制注释）

---

## 3. 关键决策与理由

### 3.1 闪屏为什么做成「第二个窗口 + 纯静态页」

- **要「极快弹出」**：走 React/Solid 首屏那条路（哪怕再快）都得等 JS bundle 下载+执行；
  `public/splash.html` 不经过打包，Vite 原样拷进 `dist/`，窗口一 show 就能画出来。
- **不进打包入口**：没有改成 Vite 多入口 —— 那会改动 chunk 命名，牵动 `check:win` 与
  产物核对这套已有机制；多入口换来的只是「闪屏也能热更新」这种我们不需要的能力。
- **随机中/英不用 IPC**：`splash.html` 里 `<img>` 之后的**内联同步脚本**在首次绘制前
  改 `img.src`。这样既不会「先出中文再翻成英文」，也不需要新窗口的 capability
  （`capabilities/default.json` 里窗口仍只列了 `main`）。
- **透明 + 无边框 + 无按钮**：人类明确要求。窗口 `transparent: true`、`decorations: false`、
  `shadow: false`；页面 `html/body` 背景全透明，图 `object-fit: contain` 铺满 4:3 窗口。
- **不抢焦点**：`focus: false` + `focusable: false` + `skipTaskbar: true` —— 它只是一张图，
  不该让用户的输入掉进一个马上要消失的窗口。

### 3.2 时序：主窗口先藏着，就绪再换

```text
启动 → 两个窗口同时创建（闪屏的 webview 趁隐藏时把图加载好）
     → setup() 里 splash.show()            ← 应用能控制的最早点
     → 主窗口在后台渲染，前端 mount 后：字体就绪 or 400ms（谁先到）→ rAF → invoke("ui_ready")
     → Rust reveal_main()：显示+聚焦主窗口、关闪屏
     → 兜底：3 秒线程（前端挂了也不会留一块墓碑）
```

- 主窗口 `visible: false` 是这条时序的一半：**不让人看到它在下面一行行渲染**。
- `reveal_main()` 写成**幂等 + 全 `Option` 守卫**：`ui_ready` 与兜底线程可能同时到；
  窗口可能不存在（Linux 开发配置整份替换了 `app.windows`，见 `AGENTS.md` §5.3 规矩 6）。
- **dev 不弹**：`tauri::is_dev()`（= `!cfg!(feature = "custom-protocol")`，已在 2.11.5 源码里核实）
  为真时关掉闪屏、直接显示主窗口 —— 起 dev 不该多等 3 秒。

### 3.3 标题栏用 `logo-small`、打包用 `logo`

20px 的标题栏图标里，带字的版本字已经完全糊了；`logo-small.png` 只有徽章，同尺寸下清晰得多。
所以两处各用各的：标题栏 `logo-small`，「关于」与 `pnpm tauri icon` 用带字的 `logo`（人类明确
「拉伸就拉伸」，不升 1024 那套商店图标）。

### 3.4 素材是**自研**，但一样登记

`THIRD-PARTY-NOTICES.md` 新增 §1b：写明 logo 与闪屏图是自研（人类 2026-09-17 确认
「图片自己做的」），随项目 AGPL-3.0-only。它**不是**第三方素材 ⇒ 不涉外部授权，
登记只为「将来有人问这个 logo 能不能用」时答案在一处。

### 3.5 `ARCHITECTURE.md` 新增**素材层**

分层检查器把 `src/assets/**` 落进了兜底的 `app` 层，于是 `shell → app` 报了违规 —— 报得没错，
它暴露的是**图片这种纯数据原本无处可归**。处理方式是在 `ARCHITECTURE.md` §1/§1.1 里正式
把 `src/assets/` 立成**素材层**：纯数据、不放代码、自己不依赖任何东西、任何层都可 import
（`scripts/check-architecture.mjs` 的 `LAYERS` 同步，每层的 `mayImport` 都带上 `assets`）。
**不是**为了让检查通过而放宽规则：素材确实没有依赖方向可言。

### 3.6 `check:win` 补闪屏资源（原先是「假绿」缺口）

原脚本只核对 `dist/index.html` 里的 `assets/*`。闪屏那两图要是没嵌进 exe，表现是
**闪屏变成一个透明空窗** —— 不报错、不明显、恰好最难查。现在：

- `splash.html` 里的图片路径是**运行时拼**的（`"splash/splash-" + lang + ".webp"`），
  所以不按字面量匹配，改为「页面里有 `splash/splash-` 拼装前缀」＋
  「`dist/splash/` 下**每一个**文件都能在 exe 里找到」。后者才是有分量的那条。

### 3.7 pi-lens 分流：4 条真修，2 条压制，其余按设计接受

真·琐碎（已修）：`src/i18n/{index,en-US}.ts` 与 `src/shell/{TitleBar,AboutDialog}.tsx`
的相对导入补 `.ts` 扩展名；`src/api/import.ts` 去掉 `unlisten` 中转变量。

压制注释（`// pi-lens-ignore: <rule>, — 理由`，写在违规行**上一行**）：`tokens.ts` 的
`MutationObserver`、`build-info.ts` 的 `__RAYBEND_BUILD__` —— 这两处检测的都是**可能不存在的全局**，
`=== undefined` 会直接 ReferenceError，`typeof` 是唯一安全写法。

**一处偏离计划**：计划里写「连字符 SVG 属性 ×8 也加压制注释」，实际做不到 —— 那 8 条报在 JSX
**开标签的属性行**上，而 JSX 无法在属性列表里插注释（`{/* */}` 只在子节点位置合法）。
剩下的选项是改用 camelCase（Solid 写的是真实 DOM 属性名，改了反而不对）或写 8 行难读的绕法。
按「纯误报就不为它改代码」处理：**接受**，理由记在这里。另外两件只可能记一笔的：
`execCommand` 的弃用提示来自 TypeScript 语言服务（不是 pi-lens 规则，注释压不掉，
而它本来就是有意的回退路径）；`getElementById`（splash.html）与 CLI 脚本里的 `console.log`
都是各自场景的正确写法。

---

## 4. 验证方式与结果（Agent 侧 = 冒烟；目视项已交人类）

| 项 | 命令 / 方式 | 结果 |
| --- | --- | --- |
| 素材逐字节核对 | `sha256sum` 源 vs 副本（×4） | 全部一致 ✅ |
| 打包图标 | `pnpm tauri icon src/assets/branding/logo.png`；读 `128x128.png` | 真 logo，非旧默认图 ✅ |
| 类型检查 | `pnpm typecheck` | ✅ |
| 单元测试 | `pnpm test` | **446 passed / 0 failed**（含新增 `uiReady` 2 条） ✅ |
| 色值门 | `pnpm lint:colors` | ✅ |
| 分层门 | `pnpm lint:arch` | ✅（新增素材层后） |
| 生产构建 | `pnpm build` | ✅ dist 含 `splash.html`、`splash/*.webp`、哈希化 logo |
| 核心库测试 | `cargo test -p raybend` | **567 + 2 passed** ✅ |
| 外壳测试 | `cargo test -p raybend-desktop` | **24 passed** ✅ |
| lint | `cargo clippy --workspace --all-targets` | 0 警告 ✅ |
| 浏览器冒烟 | `pnpm smoke:ui` | `problems: []`；闪屏页 6 条断言全绿（抽到 cn/en 各若干、src 与抽签一致、图铺满窗口、`html`/`body` 背景 `rgba(0,0,0,0)`、可交互元素 **0**） ✅ |
| Windows 产物 | `cargo build -p raybend-desktop --features custom-protocol`（`C:\rb-target\raybend`） | 01:30:29 构建 ✅ |
| 产物一致性 | `pnpm check:win` | ✅ exe 比 dist 新；4 个引用资源（外壳 + 闪屏页 + 两张闪屏图）全部命中 |
| exe 内嵌核对 | `grep -a` 查 exe | `splash.html` / `splash-cn` / `splash-en` / `logo-small` 都在 ✅ |

**未经验证（按 `AGENTS.md` §2.8，归人类）**：闪屏的实际观感（透明度是否真生效、有无边框、
是否抢焦点、随机是否真的是中英各半）、标题栏 20px logo 在两主题下的观感、Windows 上的端到端手感。
已写入 `PLAN.md` 人类验收清单 **3e / 3f**。

---

## 5. 遗留问题

1. **`design/main.pen` 里的 `AppIcon` 还是旧的占位块**（画布未同步）—— 已在 `design/main.md` §2.4
   注明；闪屏本身不进画布（它的「设计」就是那张图）。
2. **字体度量偏移 0.5–0.8px** 仍在（上一轮结论：非严重，后补）—— 见 `ASSISTANCE.md` §二。
3. **闪屏透明若在 Windows 上不生效**，那是渲染架构级风险（`AGENTS.md` §6.1/§8）；3f 就是它的
   最小验证。真出问题，退路（CEF 运行时）已在 `FUTURE.md`。
4. 本轮改动**未提交**（plannotator 流程期间不 commit，见全局规则）；工作区留 dirty 待审。
