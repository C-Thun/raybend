# 层名条平化 + flowbar 功能提示区

> 完成时间：2026-11-01 04:21:53
> 输入：崔总 2026-11-01 的两条追加要求（截图 `QQ_1793476889421.png`）

## 1. 层名条（直方图下面那行「默认」）

| 项 | 改前 | 改后 |
| --- | --- | --- |
| 外观 | `surface-track` 底色 + 圆角 4 + 左右 8px 内边距（一块小面板） | **无底色、无圆角、无内边距**（平面设计，与参数区左缘对齐） |
| 高 | 22 | **24** |
| 记号 | 无 | 左侧一枚**主色**层类型图标（`AreaGlyph`：默认=滑杆 / 全图=方块 / 控制点=圆点） |
| 文字 | 12px、`fg-2` | **13px、semibold、`fg-1`**（比别处大一档，读作「现在在改哪一层」） |
| 上下留白 | 16 / 16（直方图 `!mb-1` + 条 `mb-1` + 父级 `gap-3`） | **8 / 8**（去掉两处 `mb-1`，条上 `-my-1` 收掉 4+4） |
| 切层反馈 | 只有文字变 | 名字上**走一遍光条**（800ms 单次，`ShimmerText` + `.rb-shimmer-mask`，与导出「开始导出」处理中同一套原语） |

为什么用主色图标而不是留白：这一行的唯一职责是回答「现在在编哪一层」；主色记号 + 加粗字号是
「它是标题、不是一行灰字」的信号（崔总 2026-11-01）。

**光条的触发口径**：只在 `activeAreaId` **变化**时走一遍（首次挂载不走、改名不走、切语言不走）。
连续切层会把动画从头放（同一帧 `false → true`），否则第二次切层看不到光。

## 2. flowbar 功能提示区（`flowhint`）

```text
[导入|浏览|编辑|导出] [齿轮]      按住 Ctrl 键可拖动            RAW · 2026-11-01 …  [全屏]
                                    ↑ 居中、弹性、超出右缘渐隐、离 info 留 100px
```

* **位置**：设置按钮右边、`flowinfo` 左边，原 `SpacerFlex` 的位置（没有提示时等价于它，布局不变）。
* **宽度**：`flex-1` + `pr-[100px]` —— 吃满剩余空间，但离 info 区留 100px 不顶死。
* **文字**：居中、`fs-1`、`fg-3`；超出时右缘渐隐（`fade-x-end`，不出省略号）；进场 `rb-fade-in`。
* **状态机**：`src/lib/flow-hint.ts` —— `showFlowHint(owner, text)` / `clearFlowHint(owner)`。
  一个位置多条来源；后指到的覆盖前一条；**离开只清自己那条**（A 离开不会吃掉 B 的提示）。
  只存**成品字符串**：`lib` 不许 import `i18n`（`lint:arch`），翻译由调用方 `t()` 后传入。
* **当前唯一内容**：指着控制点小圆片 → 「按住 Ctrl 键可拖动」（离开即撤，换照片 / 关眼睛也撤）。

## 3. 涉及文件

* `src/features/editor/panels.tsx`：层名条重做（平化 + `AreaGlyph` + `ShimmerText` + 切层光条状态机）。
* `src/features/editor/area-glyph.tsx`（新）：层类型记号唯一一份（层列表行 / 拖动幽灵 / 层名条共用）。
* `src/features/editor/area-panel.tsx`：行首与幽灵行改用 `AreaGlyph`（去掉两处重复的图标分支）。
* `src/components/ui/ShimmerText.tsx`（新）：流光唯一实现（`active` / `once` / `dimBase` / `highlight`）。
* `src/styles/motion.css`：新增 `.rb-shimmer-once`（0.8s、单次）。
* `src/lib/flow-hint.ts`（新）+ `src/lib/flow-hint.test.ts`（新）。
* `src/shell/FlowBar.tsx`：提示槽位（替掉 `SpacerFlex`）。
* `src/features/editor/area-markers.tsx`：圆片 `pointerenter/leave` 提/撤提示；组件卸载时清掉。
* `src/i18n/{zh-CN,en-US}.ts`：`editor.area.ctrlDragHint`。
* `memory/COMPONENTS.md`：新增 `ShimmerText`、`AreaGlyph`、§3.2「功能提示区」。
* `AGENTS.md` §12.1 表 + §11.1 术语表（`flowhint`）。
* `specs/editor-area.md` §5.2、`design/editor.md`（11b 条）、`design/main.md`（flowbar 提示区）。

## 4. 验证

```bash
pnpm typecheck     # 通过
pnpm test          # 1380 passed / 0 failed（新增 `flow-hint` 的 4 条语义断言）
pnpm lint:colors   # ✓
pnpm lint:arch     # ✓（一次返工：`lib` 不能 import `i18n`，改成存成品字符串）
pnpm lint:i18n     # ✓
pnpm build         # ✓
```

`cargo` 侧本轮未改动（没有碰 Rust 与管线），沿用上一份记录
`implementations/2026-11-01_editor-area-bugfix-batch.md` 的 `cargo test --workspace` 结果。

**测试产物**：`pnpm build && pnpm debug:win` 已重新生成
`/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`（dist 与 exe 时间戳对齐）。

### 4.1 写完之后自查发现并修掉的两处

| 问题 | 后果 | 修法 |
| --- | --- | --- |
| 提示区的文字盒子最初**随内容收缩** | `.fade-x-end` 的渐隐落在盒子右端 ⇒ 短提示（例如「按住 Ctrl 键可拖动」）的**末尾 20px 字被淡掉**，看着像缺了尾巴 | 文字层改 `flex-1` 吃满整条带子 + `text-center`：短提示的渐隐区落在空白上；长提示才右缘渐隐 |
| 层名条的 `fade-x-end` 最初挂在**内层 span** 上 | 光条那一层是它的**兄弟** ⇒ 被光扫过的那一份不过渐隐，长名字在渐隐区外露出整段字（重影） | `fade-x-end` 提到 `ShimmerText` 的**外层盒子**上，两遍内容一起渐隐 |

## 5. 需要崔总确认

* 层名条上下各 8px 是否还有 "空得多" 的感觉（想更紧就把 `-my-1` 改成 `-my-1.5`，只动那一处）；
* 光条在名字上扫过 0.8s 会不会太快 / 太慢（`.rb-shimmer-once` 的 `animation-duration`）；
* 提示区的 100px 留白与居中位置在 1080p / 4K 下是否合适。
