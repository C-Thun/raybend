# 三个上报问题 + 一桩爆栈事故的根因追查

完成时间：2026-09-17 16:30:02 CST

范围：`src/lib/time-group.ts`、`src/features/photo-grid/PhotoGrid.tsx`、`src/features/photo-grid/viewer/Viewer.tsx`、
`src-tauri/src/source.rs`、`src/components/ui/thumb-queue.ts`、`src/features/browse/BrowsePanels.tsx`、
`src/workspaces/browse/BrowseWorkspace.tsx`、`src/i18n/{zh-CN,en-US}.ts`、`scripts/check-browse-boot.mjs`、
`crates/raybend/examples/repo-views.rs`
状态：三条修复 + 一个闸门补强，全部落盘并验证

---

## 一、人类报的三个问题

| # | 现象 | 根因 | 修法 |
| --- | --- | --- | --- |
| 1 | RAW 右上角只显示「RAW」，机身/镜头/快门全空 | `file_exif` 命令走的是**通用** EXIF 读法；上一轮补的 RAW 兜底只接到元数据 / 源目录时间 / 导入规划三处，**漏了这条** | 改用 `read_file_for`（按扩展名选读法） |
| 2 | 按时间模式下同一时间段里 RAW 整批沉到片尾 | `groupByTime` 把**片内**按 `takenAtMs` 排序，而 JPG 与 RAW 的时间来源不同（EXIF / 文件名 / mtime），秒级差异就够分层 | 片边界仍只由时间决定；**片内保持调用方传入顺序**（＝扫描顺序＝文件名自然序）；两条按旧契约写的测试重写 |
| 3 | 进看图再退出，列表跳回开头 | 打开看图时把网格**整个卸载**了，DOM 一没滚动位置就归零 | 网格保持挂载，看图改成**覆盖层**（`absolute inset-0`），退出原地不动 |

提交：`92f2b7a`（三条）+ `12686e3`（看图覆盖层原生 / 左列三态）。

## 二、那桩爆栈事故（`Maximum call stack size exceeded`）

**人类的现象**：进浏览，左侧显示「读库失败」，悬停看到 `maximum call stack size exceeded`。
（这一条其实是第 3 条的连带发现：如果没把左列的错误显示做出来，它会被上一版的 `catch {}` 静默吞掉、只显示「还没有库」。）

### 真根因

`src/components/ui/thumb-queue.ts` 的 `clear()`：

```ts
clear: () => {
  generation += 1;
  for (const entry of Object.values(entries())) { ... }   // ← 读 entries
  ...
  setEntries({});                                        // ← 写 entries
}
```

而它是在 `BrowseGrid` 的一个 effect 里被调的（`props.root` 变化 → 清缓存）。
于是那次**读被 effect 记进依赖**，紧接着的**写让这个 effect 自我失效** → 重跑 → 再读再写
（`setEntries({})` 每次都新建对象，Solid 的等值判断短路不掉）→ **无限自激直到爆栈**。

修法：那次读必须 `untrack`。代码里写明了原因，并且**解释了两个「为什么以前没炸」**：
`root === null` 时调用方 effect 提前 return（离线库 / 浏览器冒烟都走这条，所以抓不到）。

### 为什么三个既有闸门全都没看见

| 闸门 | 为什么漏 |
| --- | --- |
| `pnpm test`（Node） | `import "solid-js"` 解析到 **SSR 构建**，没有响应式，成不了环；就算显式引客户端构建，合成场景也不复现（我试了三个写法，全是假绿 —— 把修复撤掉测试照样过） |
| `pnpm smoke:ui` | 浏览器里没有后端 → 库列表恒为空 → 调用方 effect 提前 return |
| `tsc` | 只管家类型 |

### 补上的闸门

`scripts/check-browse-boot.mjs`（`pnpm check:browse`）：在浏览器里注入一个**假 Tauri 后端**
（`window.__TAURI_INTERNALS__.invoke` 返回一个**在线**的库），点进浏览，断言
「左列渲染出库名」+「无爆栈」+「无控制台错误」。

**这条检查经过双向验证**（不是假绿）：把修复撤掉 → 必红；恢复 → 必绿。

### 追查手法（值得复用）

1. 先一刀切开「后端还是前端」：写 `cargo run -p raybend --example repo-views -- <数据目录>`
   对着**人类机器上真实的 app.db + catalog.db** 跑一遍库列表（含每个库数照片的耗时）→ 后端 122ms 正常 → 问题在前端。
2. 复现：CDP 驱动 Chromium，`Page.addScriptToEvaluateOnNewDocument` 注入假后端 → 界面复现出**一模一样**的报错。
3. `Error.stackTraceLimit = 400` 后重新看堆栈 → 看到循环在 Solid 的
   `runUpdates → completeUpdates → …`（＝effect 自激），但应用帧被淹没。
4. **计数插桩 + 二分**：给可疑计算各插一个计数器（`window.__RBD`），发现 `store:*` 全是 0
   （异常发生在 `setRepository` **之前**）；再把假库改成**离线**（`root: null`）→ 不炸 → 范围压到「只有 root 非空才跑的那段」；
   最后把 `thumbs.clear()` 临时停掉 → 不炸 → 定位。
5. 数据面：WSL 里直读 Windows 的 `app.db` 会报 `disk I/O error`（9p + WAL），**拷一份再读**。

## 三、顺带做掉的两件排障基建

- **读库列表失败要留痕**：以前只在界面上显示，日志里查不到，排障只能靠人转述一句文案。
  现在同时 `console.error`（带 `// i18n-exempt: 控制台诊断`）。
- **左列三态**：`正在读库… / 读库失败（悬停看原因）/ 还没有库`。
  以前「还没读到」（每库数照片，慢盘上可能几秒）与「真的没有库」长得一模一样 —— 这次就是这么被误判成「浏览看不到库」的。

## 四、验证

```text
npx tsc --noEmit                     0 error
pnpm test                            487 passed
pnpm lint:colors / lint:arch / lint:i18n   通过
pnpm build                           通过
pnpm smoke:ui                        problems: []
pnpm check:browse                    ✓ 库非空时进浏览正常（新增；撤掉修复必红）
cargo test -p raybend                759 passed / 1 ignored（本轮未动 Rust 逻辑）
node scripts/spike-win.mjs --dry-run 通过（Windows 产物 16:29:34 含本轮全部修复）
```

**未经人类验证**：Windows 上的实际观感（看图是否真的盖满、退出是否真停在原地、左列三态文案是否可读）——
按 `AGENTS.md` §2.8 由人类目视确认。W1 计划里仍只剩第 38 步（性能数字）。

## 五、遗留

- `thumb-queue` 这条自激在 Node 里**测不出来**（已把「别再写这条测试」的说明留在测试文件里）。
  真正的守卫是 `pnpm check:browse`；等 W2 做看图/对比时，可以把它扩成完整的浏览 E2E 骨架。
- `dir_list` 在浏览左列是懒加载的（展开才读），库根那一级是选中库时读一次 —— 换库时的竞态
  （旧库的结果晚到）目前靠 `generation` 在缩略图队列里挡，目录树那边**没有**同类保护，记进 W2 再评估。
