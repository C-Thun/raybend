2026-11-12 15:23:58

# 设计稿工具由 Pencil 改为 paintress（文档与纪律同步）

## 起因

崔总指示：项目 AGENTS.md 与相关记忆文档里，**历史上用 Pencil 的记录保留**，
但**今后工作的指导**要改掉 —— 不再用 Pencil 作界面绘制工具，改用 paintress；并把 paintress 怎么用记录下来。

## 改动范围

### 新增

- `.agents/memory/PAINTRESS.md`：paintress 操作手册（本次实测撰写）
  - 定位与事实（设计文档操作层，不是编辑器；`.pen` / `.ptr`；写盘即生效）
  - **Pencil 时代限制作废清单**（必须打开编辑器 / 不能新建文件 / 人工保存 / 额度门控 / 单一写者）
  - 改稿固定顺序：`doc_info → query/get → batch|update… → validate → screenshot → 记录`
  - 12 个 MCP 工具与 CLI 命令对照表
  - 节点 schema 速查（类型、布局、文本用 `content`、图标、变量、主题）
  - 新建 / 拆分设计稿的做法（Agent 现在可以自行新建 `.pen`）
  - 截图自检（整画布 / 单帧抽板）、viewer + `#N` 短号
  - 8 条纪律与坑（备份、单写者、首次写入会重排全文、warning 不是噪音等）

### 修改（前瞻性表述）

| 文件 | 改了什么 |
| --- | --- |
| `AGENTS.md` | §2.6「Pencil 设计纪律」→「设计稿纪律（paintress）」；§2.12 画稿口径去掉 Pencil；§4 目录树加 `.agents/memory/PAINTRESS.md`；§5.1 增工具/收尾/尺寸口径三条；§10 索引加 PAINTRESS 行、设计稿行改表述 |
| `.agents/memory/DESIGN.md` | 头部定义改为 paintress 读写；§1.2 变量顺序的坑改为「已作废但顺序仍要守」；§8.4 事实更新（画布现在**可以**用数字变量），产品口径不变；§8.3 密度说明；§9.1 令牌链路第①层；§9 画布同步提示；§16 追加 2026-11-12 变更行 |
| `specs/PLAN.md` | 色彩管理行「补 Pencil 画稿」→ paintress；问题清单 #11 弹窗描边：不再等「打开 Pencil」，可直接用 paintress 补 |
| `.agents/memory/ARCHITECTURE.md` | Ark/Kobalte 决策句去掉 Pencil 限定 |
| `.agents/memory/FUTURE.md` | 智能蒙版「详细 UI 由 Pencil 定案」→ 出设计稿（paintress） |
| `.agents/memory/FUNCTION-BROWSE.md` | 「Agent 无法自行新建 `.pen`」→ 现在可以（含最简做法） |
| `.agents/memory/FUNCTION-REPOSITORY.md` | 「回到 Pencil 出稿」→ 设计稿（paintress 读写） |
| `design/main.md` | §1.1 拆分理由改写（限制消失，拆分随时可做）；§2.4 图片来源说明改为「画布工具渲染不出本地图片」；§3.6 弹窗描边欠账改指 paintress；§6 拆成 §6.0 现行 / §6.1 §6.2 历史；§9 前置说明；2026-10-24 记录里的「手动保存」标注为历史限制 |
| `design/browse.md` | §4 表头标为 Pencil 时代历史（两条已不适用、`alignItems` 一条更正）；两处「下次开 Pencil」→ paintress |
| `specs/toolsbar-compact-and-export-gallery.md` | 状态行与 §5 画稿计划改为 paintress，操作纪律改成本轮实测的做法（备份 / doc_info / validate / 截图） |
| `specs/M4-export-cleanup.md` | 未勾项的阻塞原因更新：工具已换，可直接补 |
| `docs/user-requirements.md` | 结论条目「pencil 是『指示用』」→「设计稿是『指示用』」（崔总原话引用块保持原样） |

## 关键决策与理由

1. **不把 paintress 的用法塞进 AGENTS.md**：AGENTS.md 是每次会话的纪律与索引，细节（工具表、schema、
   坑）放 `.agents/memory/PAINTRESS.md`，与 `.agents/memory/DEBUGGING.md`（工具类记忆外迁）同一做法。
2. **历史记录不回改**：`.agents/implementations/`、`specs/` 已收口单元、`design/*.md` 里带日期的现场记录一律保留；
   只改「下次要怎么做」的句子。`.agents/memory/PAINTRESS.md` §1.1 专门列了作废清单，防止后来者按旧记忆等编辑器。
3. **真相以实测为准**：以下都是本次在真实文件上跑出来的（不是照抄上游文档）：
   - `paintress validate` 四份 `.pen` 全 0 error / 0 warning；
   - `paintress` 可写 `.pen`（不只是 `.ptr`）；写入直接落盘；
   - 首次写入会把键序重排为 `version/fileToken/themes/variables/children`（语义不变，`python3` 比对相等）；
   - 数字变量在 `width` / `height` 上**会解析**（`$panel-w`→320、`$bar-title-h`→36）——
     Pencil 时代「画布不能表达密度」的前提因此失效；但「尺寸以实现为准」是崔总定的产品口径，**不变**，
     只把事实写清，避免下次拿旧理由去改政策；
   - 截图只出整张画布 → 单帧截图靠「抽板 + x/y 归零」；
   - 图标库只支持 `lucide` / `feather`，现有稿里 `Material Symbols Rounded` 共 74 个会渲染成占位框；
   - `image` 节点渲染不出真照片（占位标签）；
   - `validate` **不校验属性取值**（`alignItems: "middle"` 也 0 error），所以结构改完必须看截图；
   - 手写的节点必须**显式给 id**，否则 `query` 找不到（渲染的 `data-node-id` 也是空）；
   - `frame` 不写 `layout` 默认是 `horizontal`，要按 `x`/`y` 绝对定位就把父级设 `layout: "none"`。

## 验证

```text
paintress info      design/{main,browse,editor,export}.pen   # flavor=pen 2.21，顶层 board / 变量可读
paintress validate  design/{main,browse,editor,export}.pen   # errors 0 / warnings 0（4/4）
paintress shot      design/browse.pen → PNG                  # 整画布 3.8 s，需 /usr/bin/google-chrome
paintress render/shot 最小新建 .pen                          # 手写 JSON + validate 通过
paintress insert <file> root '<text 节点>'                   # 顶层插入，返回自动 id
paintress update  <file> <id> '{"name":…}'                   # 浅合并，未改动的键保持原值
```

未跑代码测试：本次只改文档，无源码改动（`git diff --stat` 仅 `.md`）。

## 遗留

1. `.agents/memory/FUTURE.md` 发版前板 #11（`browse.pen` / `editor.pen` / `export.pen` 弹窗帧补 0.5px 描边）：
   **崔总 2026-11-12 定：等 paintress 调顺了再改**，本轮不动（已登记，PLAN §5 #11 指向它）。
2. `design/main.md` §1.1 的拆分方案（components / shell / import 三份）现在技术上可行，未执行。

## 追加（同日，崔总答复后）

### 1. 全局 `~/.pi/agent/AGENTS.md` 同步改写

原「Pencil / pen.dev 设计稿操作（唯一通道：VS Code 扩展 / pencil MCP）」整节替换为「设计稿操作（唯一通道：paintress MCP / CLI）」：
弃用理由保留（额度门控 + 指令注入 + 加密谎报），工具换成 paintress 的 12 个工具，
纪律换成今天实测的那套（先查后改、batch 原子、validate + 看截图、备份、单写者、首次写入会重排键序、`#N` 短号、整画布截图）。
改前备份：`~/.pi/agent/AGENTS.md.bak-1794470219`。

### 2. `.ptr` 是今后格式，`.pen` 只作兼容（崔总定）

- 上游 README 原文：「`.pen`，以及它自己写出的 `.ptr`」；「`.pen` profile is frozen; `.ptr` evolves」。
- 实测：扩展名决定 `doc_info` 报的 `flavor`；**同一份 pen 结构存成 `.ptr` 照样能读能写**（`insert` 后 version/键序保持）。
- **paintress 的 MCP server 没有注入任何 instructions**（`mcp({instructions:"paintress"})` 返回 “does not provide instructions”），
  所以这条口径只能从 README 拿；已写进 `.agents/memory/PAINTRESS.md` §1 / §5 与 `AGENTS.md` §2.6 / §5.1。
- 落地：`AGENTS.md` 命名规则改为 `design/<模块>-<视图>.ptr`（新稿）+ `.pen` 兼容保留；
  `design/main.md` 拆分方案改 `.ptr`；`.agents/memory/DESIGN.md` 头部同步。

### 3. 发版前板登记

`.agents/memory/FUTURE.md`「🚀 Future Release」新增 #11：三份稿的弹窗描边，状态写明「等 paintress 调顺后再补」；
`specs/PLAN.md` §5 #11 改为指向该登记。
