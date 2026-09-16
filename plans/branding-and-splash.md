# 品牌资产落地 + 启动闪屏（M2 前的题外工作）

> 状态：**已定稿（人类 2026-09-17 答复了 4 个问题，见文末「已定」）**
> 关联：`AGENTS.md` §2.9（新依赖先讨论）、§5.3（Windows 构建与校验）、§6.1（透明挖洞的渲染风险）、`DESIGN.md` §1.1（品牌色）

## Context

**为什么做**：现在界面上没有真 logo（标题栏与「关于」用的是 Tabler 的 `IconAperture` 占位），
`src-tauri/icons/` 还是 Tauri 默认图标；也没有启动闪屏。M2（浏览）开工前把品牌部分补齐。

**手上的素材**（临时放在 `~/repos/c-thun/`，本次要复制进项目）：

| 文件 | 尺寸 | 内容 | 用途 |
| --- | --- | --- | --- |
| `logo.png` | 550×550 RGBA | 深色圆角徽章 + 薄荷色胶片双螺旋 + **琥珀色 `RayBend` 手写体** | 打包图标源（`tauri icon`） |
| `logo-small.png` | 275×275 RGBA | 同上但**没有字**（只有徽章 + 双螺旋） | 界面里的小尺寸（标题栏 20px） |
| `splash_v1-cn.webp` | 1086×814 RGBA | 胶片/拍摄中的孩子/古建漏窗拼贴 + 「光伴」+「你的理想摄影伴侣」+ 徽章 | 闪屏（中文） |
| `splash_v1-en.webp` | 1086×814 RGBA | 同构图英文版 | 闪屏（英文） |

两张 splash 都≈ 4:3（1086/814 = 1.3342），**约 25% 的像素是透明的**（不规则边缘 + 羽化），
非透明区 bbox 到 (1073, 814) —— 即「图本身不是矩形」，必须用**透明窗口**承载。

素材配色与现有品牌令牌一致（薄荷 ≈ `--brand #52c6ab`、琥珀 ≈ `--brand-2 #f0b033`），无需改色。

## 现状勘查（已确认）

- 标题栏：`src/shell/TitleBar.tsx` 里是 `<div class="size-5 rounded-ui bg-brand"><IconAperture size={13}/></div>` —— 换成 `logo-small`。
- 「关于」弹窗：`src/shell/AboutDialog.tsx` 也用 `IconAperture size={22}` —— 换成大 logo（那里地方大，可以显示带字版本）。
- 打包图标：`src-tauri/tauri.conf.json` 的 `bundle.icon` 指向 `src-tauri/icons/*`（目前是 **Tauri 默认图标**）。
- 窗口：`tauri.conf.json` 只声明了 `main`（1600×1000，`decorations: false`）；`tauri.linux.conf.json` **重复了整份数组**（Tauri 的合并是 RFC 7396，**数组整体替换** —— 见 `AGENTS.md` §5.3 第 6 条）。
- capabilities：`src-tauri/capabilities/default.json` 的 `windows: ["main"]` —— 新窗口**默认没有任何权限**（闪屏页不需要权限，正好）。
- 前端：还没有 `public/` 与 `src/assets/` 目录；`vite.config.ts` 未改 `publicDir`（默认 `public/` ✓）。
- Rust：`src-tauri/src/lib.rs` 的 `setup()` 里目前只做数据底座预热；已有 `MAIN_WINDOW_LABEL` 常量。
- Tauri 版本 **2.11.5**（crate）：`WindowConfig` 支持 `focus / focusable / transparent / skipTaskbar / shadow` ✓；`tauri::is_dev()` 存在 ✓（`tauri-build` 还提供 `cfg(dev)` 别名）。

## Approach（推荐方案）

### 1. 资产落位

```text
src/assets/branding/logo.png          ← 打包图标源（`tauri icon` 的输入）
src/assets/branding/logo-small.png    ← 界面小图标（Vite 会哈希后进 dist）
public/splash.html                    ← 闪屏页（纯 HTML+内联 CSS，**零 JS bundle**）
public/splash/splash-cn.webp          ← 闪屏图（`public/` 原样进 dist，Tauri 会把整个 dist 嵌进 exe）
public/splash/splash-en.webp
```

为什么闪屏放 `public/` 而不是 vite 多入口：闪屏要**最快**，多入口会引入 bundle 与 chunk 命名变化；
`public/` 是原样拷贝，页面只有一张本地图。`src/assets/` 放界面用的 logo（走哈希，便于缓存与类型）。

### 2. 界面里换 logo

- `TitleBar.tsx`：`<img src={logoSmall} class="size-5" alt="" aria-hidden="true" />`
  —— **不额外加 `rounded-ui`**：徽章自己的圆角在 20px 下约 2.4px，再套 4px 的 CSS 圆角会切进徽章边缘。
- `AboutDialog.tsx`：换成大 logo（`size-12` 或 `size-14`），保留原有版本/通道/git 信息。

### 3. 打包图标

`pnpm tauri icon src/assets/branding/logo.png` → 重新生成 `src-tauri/icons/*`（含 `.ico` / `.icns` / 商店图）。
`tauri.conf.json` 的 `bundle.icon` **路径不变**，无需改配置。

> ⚠️ 已知限制：源图 550×550，Windows 实际用到 256 ✓ 够；但生成 512/1024 的商店图是**放大**。
> 若有更大的（≥1024）或矢量的源，换掉即可（见文末「待你顺手确认」，不阻塞开工）。

### 4. 闪屏（透明、4:3、无按钮、随机中/英）

**窗口**（加进 `tauri.conf.json` 的 `app.windows`；尺寸按人类选定 **800×600**）：

```jsonc
{
  "label": "splash",
  "url": "splash.html",
  "width": 800, "height": 600,        // 4:3，与图片等比
  "resizable": false,
  "decorations": false,               // 无边框、无按钮
  "transparent": true,                // 关键：图的不规则边缘要真的透出桌面
  "shadow": false,
  "center": true,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "focus": false, "focusable": false, // 不抢焦点（主窗口 reveal 时再 set_focus）
  "visible": false                    // 由 setup 里 show()，避免任何闪白
}
```

**时序**（人类选的是「藏到就绪再一起出现」）：

```text
启动
 ├─ Tauri 并行创建两个窗口：splash（隐藏，但 webview 已在加载那张图）、main（visible: false）
 ├─ setup(): splash.show()                    ← 应用能控制的最早点
 ├─ 主窗口在后台渲染（用户看不见）
 ├─ 前端首屏挂载 + 字体就绪 → invoke("ui_ready")
 └─ Rust: main.show() + main.set_focus() + splash.close()

兜底：3 秒没等到 ui_ready → 照常 reveal（宁可少一个闪屏，也不能把用户卡在闪屏上；
      前端挂掉时这条路径就是「和现在一样，直接看到界面」）
```

主窗口必须在 `tauri.conf.json` 里加 `"visible": false` —— 否则闪屏期间能看到它在下面渲染。

**开发模式不弹且不等**：`tauri::is_dev()` 在 2.11.5 里的定义正是
`!cfg!(feature = "custom-protocol")` ——
`pnpm tauri dev`（走 Vite dev server，不加那个 feature）→ `true`；Windows 产物与打包版 → `false`。
所以 dev 分支做两件事：**关掉 splash 窗口**（省一份 webview）、**立刻 show 主窗口**
（否则每次起 dev 都要等 3 秒兜底，开发体验倒退）。

> **Linux 开发配置正好省事**：`tauri.linux.conf.json` 重复了整份 `app.windows`（Tauri 的合并是 RFC 7396，
> **数组整体替换** —— `AGENTS.md` §5.3 第 6 条），所以 WSL 侧的 dev 构建里**根本没有 splash 窗口**、
> 主窗口也没有 `visible: false` —— 与「dev 不弹」的目标一致。`lib.rs` 里一律用
> `get_webview_window(...)` 的 `Option` 判断，窗口不存在时静默跳过。
> Linux 产品版要不要闪屏属于远期（`FUTURE.md` 的平台扩展），本次不碰。

**随机中/英**：`public/splash.html` 里一段**内联同步脚本**，在首次绘制前把 `<img>` 的 `src`
设成 cn 或 en（`Math.random() < 0.5`），并把结果记在 `<html data-splash="cn|en">` 上便于排查。
脚本放在 `<img>` **之后**（body 末尾），此时元素已在、且仍在首次绘制前：不闪白、不发生两次下载、
不需要 IPC、不需要任何窗口权限（`capabilities` 不用动，闪屏页不碰 Tauri API）。

页面骨架（实现时照抄）：

```html
<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <title>RayBend</title>
  <style>
    html, body { margin: 0; height: 100%; background: transparent; overflow: hidden; }
    body { display: flex; align-items: center; justify-content: center; }
    /* 图是**不规则边缘**的透明图：铺满窗口、不裁剪、不拖拽、不吃事件 */
    img { width: 100%; height: 100%; object-fit: contain;
          -webkit-user-drag: none; user-select: none; pointer-events: none; }
  </style>
</head>
<body>
  <img id="splash" alt="" />
  <script>
    (function () {
      var lang = Math.random() < 0.5 ? "cn" : "en";   // 每次启动随机
      document.documentElement.dataset.splash = lang;  // 便于排查（F12 / 冒烟）
      var img = document.getElementById("splash");
      img.src = "splash/splash-" + lang + ".webp";
      img.alt = lang === "cn" ? "光伴" : "RayBend";
    })();
  </script>
</body>
</html>
```

**顺带的收益**：闪屏是**「Windows 上 webview 能不能真透明」**这条 M0 风险项
（`AGENTS.md` §6.1 / §8 第 2 项）的**真机最小验证** —— 透明窗口一旦在 Windows 上不工作，
闪屏会立刻暴露（一片黑底/白底），比将来在渲染方案里才发现便宜得多。

### 5. 顺带：pi-lens 报告的分诊结果（4 条真·琐碎 + 压制）

**压制注释的格式约定**（项目里 `pi-lens-fix` 技能的坑）：规则 id 必须**逗号分隔**、理由跟在逗号之后，
注释放在报错行本身或**紧邻上一行**：

```tsx
{/* pi-lens-ignore: hyphenated-svg-attribute, — Solid 写的是真实 DOM 属性名，SVG 呈现属性本来就是连字符
    （Tabler 自己的 svg 工厂也这么写）；改 camelCase 会让 SVG 不认 */}
```

`pnpm smoke`/`lens` 报的 82 条里，**只有 4 条是真的**（都很小），其余都是误报或设计使然，
详见本文件末尾附录。**人类已拍板：本次一并修掉（1 行×4）并给设计使然的几处加 `pi-lens-ignore` 注释**（格式与理由写法见上一段）。

### 6. 接线草图（实现时照抄）

**Rust（`src-tauri/src/lib.rs`）**：

```rust
/// 闪屏窗口标签（与 `tauri.conf.json`、`capabilities/` 对应）
pub const SPLASH_WINDOW_LABEL: &str = "splash";
/// 等界面就绪的上限：超时就直接显示主窗口 —— 宁可少一个闪屏，也不能把用户卡住
const SPLASH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// 收尾：显示主窗口（拿回焦点）+ 关掉闪屏。**幂等**（超时线程与 ui_ready 可能都调它）。
fn reveal_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(main) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = main.show();
        let _ = main.set_focus();
    }
    if let Some(splash) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
        let _ = splash.close();
    }
}

/// 前端首屏（含字体）就绪 —— 由 `src/App.tsx` 在 onMount 后调一次。
#[tauri::command]
fn ui_ready(app: tauri::AppHandle) {
    reveal_main(&app);
}
```

`setup()` 里追加（挨着现有的数据底座预热）：

```rust
use tauri::Manager;
if tauri::is_dev() {
    // 开发模式：不弹闪屏，而且**主窗口立刻可见**（否则每次起 dev 都要等 3 秒兜底）
    if let Some(splash) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
        let _ = splash.close();
    }
    if let Some(main) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = main.show();
    }
} else {
    if let Some(splash) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
        let _ = splash.show();
    }
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        std::thread::sleep(SPLASH_TIMEOUT);
        reveal_main(&handle);
    });
}
```

别忘了把 `ui_ready` 加进 `generate_handler![...]`（`lib.rs` 里有个测试盯着「状态类型都 manage 了」，
新增命令同理要能一眼查到）。

**前端（`src/api/window.ts` + `src/App.tsx`）**：

```ts
// api/window.ts：照现有风格（懒加载 + 浏览器里静默降级）
export async function uiReady(): Promise<void> {
  if (!isTauriRuntime()) return;
  const core = await (coreModule ??= import("@tauri-apps/api/core"));
  await core.invoke("ui_ready");
}
```

```ts
// App.tsx：首屏 + 字体就绪后通知 Rust。**限时**：字体加载慢也不该把闪屏拖住。
onMount(() => {
  const fonts = "fonts" in document ? document.fonts.ready.catch(() => undefined) : Promise.resolve();
  void Promise.race([fonts, new Promise((r) => setTimeout(r, 400))]).then(() =>
    requestAnimationFrame(() => void uiReady()),
  );
});
```

## Files to modify

| 文件 | 改什么 |
| --- | --- |
| `src/assets/branding/logo.png`、`logo-small.png`（新） | 两个 logo（原图照抄，不转码） |
| `public/splash/splash-cn.webp`、`splash-en.webp`（新） | 两张闪屏图（原图照抄） |
| `public/splash.html`（新） | 闪屏页：透明背景、无 UI、body 末尾内联随机选图 |
| `src/shell/TitleBar.tsx` | 占位图标 → `logo-small` |
| `src/shell/AboutDialog.tsx` | 占位图标 → 大 logo |
| `src/api/window.ts` | 新增 `closeSplash`/`uiReady` 的前端封装（含 `isTauriRuntime()` 降级） |
| `src/App.tsx` | `onMount` 里通知 Rust「界面好了」 |
| `src-tauri/tauri.conf.json` | 新增 `splash` 窗口声明 + 主窗口加 `"visible": false` |
| `src-tauri/tauri.linux.conf.json` | **不用改** —— 它重复了整份 `app.windows`（数组整体替换），于是 Linux dev 里没有 splash 窗口、主窗口也保持可见，正好就是「dev 不弹且不等」 |
| `src-tauri/src/lib.rs` | `setup()` 里 `show()` 闪屏 + 兜底线程；新命令 `ui_ready` |
| `src-tauri/icons/*` | 由 `pnpm tauri icon` 重新生成 |
| `design/main.md`、`DESIGN.md` | 记一笔：标题栏图标换成真 logo；闪屏的规格 |
| `THIRD-PARTY-NOTICES.md` | 若 logo/splash 是外部素材（AI 生成或委托），注明来源与授权 |
| `src/i18n/index.ts`、`src/i18n/en-US.ts`、`src/api/import.ts` | pi-lens 的 4 条真·琐碎（补 `.ts` 扩展名 ×3、去掉中转变量 ×1） |
| `src/components/ui/Form.tsx`、`RemoveButton.tsx`、`tokens.ts`、`src/lib/build-info.ts`、`src/lib/clipboard.ts` | 给「误报但每次都刷屏」的几处加 `// pi-lens-ignore:` 注释（含理由） |

## Reuse

- `src/api/window.ts` —— 已经封装了窗口操作的懒加载 + 浏览器降级（`isTauriRuntime()`），新命令照它的样子写。
- `src-tauri/src/lib.rs` 的 `MAIN_WINDOW_LABEL` 常量与 `setup()`（数据底座预热已在里面，新逻辑挨着它写）。
- `src/lib/build-info.ts` —— 「关于」弹窗已经在用，不必动。
- Vite 默认 `publicDir: "public"` —— 无需改 `vite.config.ts`。
- `scripts/check-win-artifact.mjs`（`pnpm check:win`）—— 产物校验照旧；`dist` 多了 `splash.html` 与两张图不影响它。

## Steps

- [x] 1. 复制四个素材进仓库（logo 进 `src/assets/branding/`、splash 两张进 `public/splash/`），
      `sha256sum` 对照「源文件 / 仓库内文件」确认字节一致（大文件别被转码）
- [x] 2. 生成打包图标：`pnpm tauri icon src/assets/branding/logo.png`，检查 `src-tauri/icons/` 变化
- [x] 3. 标题栏 + 「关于」换成真 logo（小/大），两档主题各看一次观感
- [x] 4. 写 `public/splash.html` + 两张图（透明、无按钮、head 内联随机选图、禁拖拽）
- [x] 5. `tauri.conf.json` 加 `splash` 窗口 + 主窗口 `"visible": false`（Linux 配置**不改**，理由见 §4）
- [x] 6. `lib.rs`：setup 里 `show()` 闪屏 + `ui_ready` 命令（关闪屏 + 显示主窗口）+ 3 秒兜底
- [x] 7. `src/api/window.ts` + `App.tsx`：首屏挂载后调 `ui_ready`（浏览器里静默降级）
- [x] 8. 单测：随机选择（若抽成纯函数）、`ui_ready` 的降级路径；冒烟如需调整（dev 不弹则不用）
- [x] 9. 文档：`design/main.md` §2.1（标题栏图标）、`DESIGN.md` 变更记录、`THIRD-PARTY-NOTICES.md`（素材来源）
- [x] 10. pi-lens：修 4 条真·琐碎 + 给设计使然的加压制注释，重扫确认警告数下降
- [x] 11. 收尾：`pnpm build` → Windows 产物重建 → `pnpm check:win` → 写 `implementations/` 记录
      → 把「闪屏目视项」写进 `PLAN.md` 的人类验收清单

## Verification

#### Agent 侧（冒烟/程序化）

- `pnpm typecheck && pnpm test && pnpm lint:colors && pnpm lint:arch && pnpm build` 全绿
- `pnpm build` 之后 `dist/` 里必须有：`splash.html`、`splash/splash-cn.webp`、`splash/splash-en.webp`、
  以及哈希后的 `logo-small-*.png`；`pnpm check:win` 仍 ✓（exe 比 dist 新、资源名命中）
- `cargo test -p raybend-desktop`（含既有的「状态类型都 manage 了」那条）通过
- dev 下：`pnpm tauri dev` **不弹闪屏**，且主窗口**立刻**可见（不许被 3 秒兜底延迟）
- dev 下 `pnpm smoke:ui` 保持 `problems: []`（没有多出来的 CDP 页面目标）

#### 人类侧（目视 —— 闪屏是视觉产物，按 `AGENTS.md` §2.8 归 E2E）

- 双击 `raybend-desktop.exe`：闪屏（**800×600**）**先**出现（感觉上是「立刻」），
  主窗口就绪后**闪屏消失 + 主窗口出现**（在此之前主窗口不该露出来）
- 连开 5~6 次：中/英**随机**出现（不是总同一张）
- 闪屏窗口：**没有边框、没有按钮、没有阴影**，图片边缘的不规则形状**没有被矩形背景包住**
  （这条同时验证了「Windows 上 webview 真透明」——见 `AGENTS.md` §6.1 的风险项）
- 任务栏里**不该**出现闪屏那一项；闪屏**不该**抢走键盘焦点
- 标题栏左上角图标：小尺寸下仍然可辨认（这正是 `logo-small` 的用途）；深/浅两主题各看一眼
- 开始菜单/桌面快捷方式/任务栏的图标换成了新 logo（打包安装后）

## 已定（人类 2026-09-17 答复）

1. **主窗口时机**：闪屏期间主窗口**藏到就绪**，`ui_ready` 时一起出现（见 §4 时序）。
2. **闪屏尺寸**：**800×600**（4:3，比推荐的 640×480 更醒目 —— 人类选择）。
3. **dev 是否弹**：**只在产物里弹**（`tauri::is_dev()` 判断，见 §4）。
4. **pi-lens**：**一并修掉那 4 条真·琐碎，并给设计使然的加压制注释**（见步骤 11 与附录）。

## 待你顺手确认（不阻塞开工）

- 打包图标源是 **550×550**：Windows 实际用到 256 ✓ 够；但 `tauri icon` 生成的 512/1024（商店图）
  是**放大**的。若有 ≥1024 或矢量版（SVG/AI），我换掉源图重新生成即可。
- 素材授权/来源：如果 logo 与 splash 是外部委托或 AI 生成，给我一句出处与许可，我写进
  `THIRD-PARTY-NOTICES.md`（若是你自己做的，写「自研，随项目 AGPL-3.0-only」也可以）。

---

## 附录：pi-lens 82 条的分诊（结论：**没有 error，只有 4 条真·琐碎**）

先说明一点：**会话缓存与工作区扫描里都没有 `error` 级别的诊断**
（`lens_diagnostics severity=error` → "No diagnostics found"）。
你看到的「一个错误」应当是 turn-end 汇总里的 🔴（它把 warning 也计进去）或编辑器里的 hint 图标。

| 组 | 条目 | 判定 |
| --- | --- | --- |
| `hyphenated-svg-attribute` ×8（Form/RemoveButton） | 说 SVG 属性该用 camelCase | **误报**。Solid 写的是真实 DOM 属性名，SVG 呈现属性本来就是连字符；**Tabler（Solid 生态的图标库）自己的 svg 工厂里就写着 `'stroke-width': 2`**。改成 camelCase 反而会让 SVG 不认 |
| `no-typeof-undefined` ×2 | 建议 `=== undefined` | **误报，且照改会引入真 bug**：`typeof MutationObserver === "undefined"` / `typeof __RAYBEND_BUILD__ === "undefined"` 是检测**未声明全局**的唯一安全写法，直接比较会在 Node 测试里 ReferenceError |
| `prefer-at`（rows.ts:122） | 建议 `.at(-1)` | **误报（建议不可编译）**：tsconfig `lib: ["ES2020","DOM"]`，`Array.prototype.at` 属 ES2022 → 照改会 TS2550 |
| `ts-redundant-filter-map`（store.ts:267） | 说 filter+map 冗余 | **误报**：规则找的是 `.filter(x => x)`，这里是 `.filter(item => item.takenAtSource !== "exif").map(itemId)`，谓词有意义 |
| `no-non-null-assertion` ×2（viewer/store.ts） | 非空断言 | **误报**：两处都在 `if (list.length === 0) return;` 之后且下标已 clamp |
| `switch-without-default`（build-info.ts） | switch 缺 default | **误报**：穷尽 union + 每个 case 都 return，strict 下新增成员会红；影响面仅是调试标签 |
| `no-console-except-error` ×2（easy-destroy.ts） | 说别用 console | **误报**：那两行**就是** `console.error`（catch 里的降级日志） |
| `no-conditional-empty-object-spread` ×5 | 条件展开 | **设计使然**：为 `exactOptionalPropertyTypes` 不传 `undefined`（代码里有注释） |
| `no-unknown-parameters` / `no-runtime-typeof`（十余条） | 说「应在 I/O 边界解码」 | **误报**：这些函数**本身就是边界解码器**（`errorText(unknown)`、`normalizeTheme(unknown)`、`pickDirectory` 认插件返回值） |
| `inline-styles`（十余条） | 说该用 CSS module | **设计使然**：Solid+Tailwind 里动态令牌值（`--tile-cell`、`aspect-ratio`、行高）只能内联 |
| `no-known-value-widening`（en-US.ts） | 值拓宽 | **设计使然**：`Record<MessageKey, string>` 是**中英对齐**的类型闸门（`locale-parity.test.ts` 盯着） |
| `typos: pendings` ×6 | 拼写 | **忽略**：测试替身里「一批 pending 请求」的取名 |
| `may be converted to async` ×2（db.ts/import.ts） | 可改 async | **风格提示**：那是 `m ??= import(); return m.then(...)`，改 async 只会多包一层 promise |
| `execCommand deprecated`（clipboard.ts） | 已废弃 API | **设计使然**：非安全上下文下唯一的复制通道（代码里注明） |
| **真·琐碎 4 条** | `find-import-file-without-extension` ×3（i18n 两处 + en-US 一处没写 `.ts`）、`redundant-state` ×1（`api/import.ts` 的 `unlisten` 中转变量） | **可修**（1 行×4），与品牌工作无关，可选 |
