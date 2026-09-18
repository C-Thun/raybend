# 统一取图口 + 看图接进浏览（W2 阶段 1 的 1.0–1.2）

完成时间：2026-09-18 19:42:11 CST

计划：`plans/M2-W2.md` 阶段 1 的 1.0 / 1.1 / 1.2（口径见 §2.1）
相关：`AGENTS.md` §6.1（渲染接口纪律）、`BROWSE.md` §5.3–§5.6、`design/browse.md` §2.5

---

## 一、1.0 统一取图口（Rust 侧）

新增 `crates/raybend/src/display/mod.rs` —— **服务 view 与缩略图缓存的取图总入口**，
是对「编辑器里真正把图片渲染出来那条链路」的封装（人类 2026-09-18 的原话见 §2.1）。

| 东西 | 说明 |
| --- | --- |
| `ImagePurpose` | `grid`(384) / `strip`(192) / `screen`(1920) / `original` |
| `DisplayImage` | `bytes` + `mime` + `origin`(OriginalFile/Rendered) + `backend`(Bitmap/Raw) + `size` |
| `EditSpec` | **编辑栈的占位**（`has_edits`）：结构留好，编辑里程碑接进来；两个后端都要在同一处应用它 |
| `BitmapBackend` | **没编辑 + `original` → 直接给原文件字节**（「直接给原图地址」那条路）；其余走渲染管线 |
| `RawBackend` | **永远走渲染管线**（内嵌预览优先 → 解码），不存在「给原图」 |
| `display_image()` | 按 `media::kind` 分派 —— **调用方不需要知道是 RAW 还是位图** |

* 复用 `thumbnail::{render_file, SizeClass}`（RAW 走的还是 worker 进程隔离那条路），
  渲染输出统一 JPEG；错误语义：认不出/解不开 → `Ok(None)`，IO 失败 → `Err`。
* **不落临时文件**：所有路径都是「现算 + 交给既有缓存」，没有自建临时图。
* 7 条单测（用途解析、MIME 判定、原图直给、**编辑过的位图不许走原图捷径**、**RAW 永远不给原文件**…）。

**接线**：`src-tauri/src/thumbs.rs` 新增 `view_image(path, purpose)` 命令（返回裸字节，参照 `thumb_get`），
在 `src-tauri/src/lib.rs` 注册；`src/api/db.ts` 新增 `getViewImage()`。
**两个看图入口都改走它**（`photo-grid/PhotoGrid.tsx` 与浏览工作区）—— 于是这个口真的被用起来了，
不再是「建了没人用」的架子。

## 二、1.1 看图件搬成共享件

`src/features/photo-grid/viewer/` → **`src/components/ui/viewer/`**（`git mv`，历史保留）。

为什么放 `components/ui`：架构规则**不允许 feature 之间互相 import**
（`ARCHITECTURE.md` §1 / `check-architecture.mjs` 规则 2），而看图现在有导入网格与浏览工作区两个消费方；
放基础组件层两边都能用，也不必为它单开一层。代价（也是纪律）：`components/ui` **不许 import `api`**，
所以取图能力仍由调用方注入（`ViewerStoreDeps.loadScreen/loadThumb`）—— 正好对上「取图策略在 Rust 侧」。

## 三、1.2 看图接进浏览

| 文件 | 改动 |
| --- | --- |
| `features/browse/BrowseGrid.tsx` | 新 prop `onOpenViewer(photos, index)`；新增 `orderedPhotos()`（**显示顺序**，含分组后的片内顺序）与 `openViewerAt()`；tile 上挂 `onDblClick`；网格容器接 `onKeyDown`（回车进看图，且只在**恰好选中一张**时） |
| `workspaces/browse/BrowseWorkspace.tsx` | 建共享看图 store（`loadScreen` → `getViewImage(path,"screen")`，`loadThumb` → `getThumbBytes(path,"grid")`）；中列加 `relative` + `<Show when={viewer.state().active}><Viewer/></Show>`；打开看图时**顺手收掉展开的库列表**（`BROWSE.md` §4.2 的第二个收起条件） |

* 退出（`Esc`/回车）、`←`/`→` 切图由看图件自己的键盘处理负责（它监听 window，仅在 active 时生效）✓
* 网格**不卸载**（滚动位置留得住 —— M2-W1 踩过「看图回来回到列表开头」那个坑）。

## 四、验证

```text
cargo test -p raybend                785 passed / 1 ignored（新增 display 7 条）
cargo clippy --workspace --all-targets   0 warning
npx tsc --noEmit                     0 error
pnpm test                            525 passed
pnpm lint:arch / lint:colors / lint:i18n   通过（搬动后分层仍然合规）
pnpm build                           通过
pnpm check:browse                    通过
```

## 五、冒烟桶到的两个真 bug（都修了）

把「双击进看图 / Tab 三态 / Esc 退回」写成断言后，一下子桶出两个 bug：

### 1. `onDblClick` 挂在 `<Tile>` 上**不会生效**

现场：「点击生效（选中 1 张）、双击毫无反应」。`Tile` 把 `onClick` 接管进自己的 `splitProps`
（它有一套激活语义），而 `onDblClick` 经由 props 传进去后**没有被挂到 DOM 上**。

修法：在 `Tile` 外面包一层 `div`（`class="contents"`，不产生盒子、不影响网格布局）挂 `onDblClick` ——
与导入网格 `PhotoGrid` 一直以来的做法一致（它也是包一层）。

### 2. 回车打开看图后被**同一个事件**立即关掉

现场：`viewer.state().active` 在 show() 之后是 `true`，下一个微任务已经变回 `false`。
根因：网格的回车处理器打开看图 → `Show` 同步挂载看图件并给它 `window` 挂上键盘监听 →
**同一个仍在冒泡的 keydown** 到达 window → 看图件把它当成「回车＝退出」→ 关掉。

修法（两层）：

* `BrowseGrid.onGridKeyDown`：处理完后 `event.stopPropagation()`（这个回车已经被用掉了）；
* `Viewer` 的键盘处理器：开头加 `if (event.defaultPrevented) return;` —— 通用护栏，
  以后胶片带/命令面板用回车打开看图时不会重踩。

> 教训：把交互写成**断言**（而不是靠人眼扫一眼）才抓得到这类「一个事件被两处消费」的 bug；
> 两条断言此后会一直守着它们。

## 六、冒烟断言的覆盖面（`pnpm check:browse`）

fixture 从「0 张照片」扩成「3 张带标记的照片 + 1×1 PNG 字节（`thumb_get`/`view_image` 都回它）」，
新增断言：

| 断言 | 守的是什么 |
| --- | --- |
| 选中库 / 未选目录时网格是空态、不发查询 | 阶段 0.2 的口径（不能退回去） |
| 树首层无 `photos`、无 `_RAW` | 阶段 0.2 的口径 |
| 5 个库时出现「查看所有库」、点开后全部库可见、目录树仍在 | 阶段 0.8 的库列表规则 |
| **双击 → 看图件打开 + 状态栏出现** | 1.2 / 1.4 |
| **Tab → ①↔②↔③ 循环（左右列/胶片带标签同步）** | 1.3 |
| **Esc → 看图件关掉、左右栏必定回来** | 1.3 |

## 七、验证

```text
cargo test -p raybend                785 passed / 1 ignored
cargo clippy --workspace --all-targets   0 warning
npx tsc --noEmit                     0 error
pnpm test                            530 passed
pnpm lint:arch / lint:colors / lint:i18n   通过
pnpm build                           通过
pnpm check:browse                    通过（含上面 6 类断言）
```

## 八、遗留（属后续步骤）

* `Tab` 第③态的**胶片带**本体还没做（步骤 2.1）—— 状态机与 `data-film` 已经就位。
* 右栏在看图态要换成「预览 + 视野框 + 直方图」（步骤 1.6）。
* 看图件里 `Tab` 走到 `no-sides` 时**顶部三条不变**（符合设计）—— 冒烟只断言了两侧列，没有断言顶部。

* **看图在浏览里的冒烟断言**还没加：`check:browse` 的假后端现在给 0 张照片，
  双击打开的路径测不到。等 1.5（渐进显示）时把 fixture 扩成「有照片 + 有缩略图字节」，
  再加「双击 → `[data-viewer="open"]` 出现」的断言。
* 选项卡（`Tab` 三态）、底部状态栏、右栏看图态、胶片带/对比都是 1.3–2.5 的事。
