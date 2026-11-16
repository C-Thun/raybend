# 设计稿工具：paintress（.agents/memory/PAINTRESS.md）

> **2026-11-12 起，界面设计稿一律用 paintress 读写；不再使用 Pencil（pen.dev 编辑器 / CLI）。**
> 本文件是 paintress 的操作手册：工具与 CLI 对照、节点 schema 速查、视觉自检、人机协作与纪律。
> 设计稿**内容**记在 `design/*.md`；视觉规则见 `.agents/memory/DESIGN.md`；设计纪律见 `AGENTS.md` §5.1。
>
> 依据：2026-11-12 在 `design/main.pen` / `browse.pen` / `editor.pen` / `export.pen` 上实测（paintress 0.2.3）；
> 同日（21:11）升到 0.4.0（`new` / `info` / `validate` / `render` / `shot` / MCP `paintress_new` 全通），
> 次日 03:59 再升到 **0.5.1**（`shot --board` 单画板截图实测：`main.pen` 的 `rJRpX` 截出 1600×1000，不再是整张 19360×4036 画布）。

---

## 1. 它是什么 / 不是什么

| 是 | 不是 |
| --- | --- |
| 设计文档**操作层**：读、查、改、校验 `.pen` / `.ptr`（明文 JSON） | 不是编辑器：没有画布、属性面板、拖拽 |
| 把文档渲染成 HTML 与 PNG（Agent 用截图自检） | 不替你决定界面长什么样 |
| 本地 viewer：人看图、点节点，回传 `#1 #2 …` 短号 | 不是「画图工具」，结构由 Agent 写 JSON 表达 |

- 版本 `0.5.1`，`paintress` 已在 PATH（全局 npm 安装，`npm install -g paintress` 只在缺失时执行；升级用 `npm install -g paintress@latest`，升完把包内 `skills/paintress/SKILL.md` 覆盖到 `~/.pi/agent/skills/paintress/SKILL.md`）。
- pi 的 MCP 入口已在 `~/.pi/agent/mcp.json`（`{"command":"paintress","args":["mcp"]}`），
  用 `mcp({ server: "paintress" })` 看工具；`paintress doctor` 可核对各客户端登记状态。
- 格式两种：`.pen`（pen flavor `2.21`，**schema 冻结**，**只作兼容**）与 `.ptr`（paintress 自有，**会演进，新建文件一律用它**）。
  扩展名决定 `doc_info` 报的 `flavor`；两种都能读能写（实测同一份内容存成 `.ptr` 照样 `validate` 通过、可继续 `insert`）。
  仓里 4 份 `.pen` 全部 `validate` 通过（0 error / 0 warning），按兼容格式继续维护。
- **写入直接落盘**：没有「保存」这一步，也没有编辑器 buffer。

### 1.1 Pencil 时代的限制全部作废（别再按旧记忆干活）

| 旧记忆（Pencil） | 现在（paintress） |
| --- | --- |
| 必须在 VS Code 里打开目标 `.pen`，否则 IPC 报错 | 不需要打开任何编辑器，给文件路径就行 |
| Agent 无法新建 `.pen`，要人先建好 | **可以**：`paintress_new` / `paintress new <file>` 直接产出合法 `.ptr`（0.4.0 起是第一选择）；要预置结构再手写 JSON 或复制现文件删掉旧 board |
| 改完在内存里，磁盘保存必须由人执行 | 改完就在磁盘上，`git diff` 立刻可见 |
| 额度门控（每账单周期 5 个活跃日）会封掉全部工具 | 无额度、无账号、无云端依赖（本地 CLI + 本机 Chromium） |
| 同一时刻只能有一个写者（编辑器 buffer 会覆盖） | 仍要守单写者，但对的是**磁盘**：别让人同时在别的工具里编辑同一文件 |

> 历史文件（`.agents/implementations/`、`design/*.md` 里带日期的记录、`specs/` 已收口单元）里的 Pencil
> 记述是**当时的现场**，不回改；遇到「下次开 Pencil 时补」这类**未完成**事项，现在用 paintress 补。

## 2. 每次改稿的固定顺序

```text
① doc_info       摸清顶层 board / 变量 / 节点类型分布（不猜「第二个屏幕」是哪块）
② query / get    拿真实节点 id 与现有属性（绝不凭空造 id）
③ batch 或 update/insert/delete/move   改（batch 原子：任一步失败整体作废）
④ validate       结构错误（errors）+ 语义警告（warnings：重复 id / 悬空 ref / 未知类型 / 未定义变量）
⑤ screenshot     自己看图（render 的 HTML 给人看）
⑥ 记录           实施记录里写清帧名 + 节点 id
```

- **先查后改**：`paintress_query` 按 `type` / `name`（精确或正则）/ `id` / `reusable` 找；
  只有带 `id` 的节点会被 query 返回 —— 手写的节点要**显式写 id**（见 §5）。
- **`#N` 是人在 viewer 里点的**：`paintress_marks` 读短号 → 节点 id 的映射，
  用户说「#1 改成蓝色」时先读 marks，不要按视觉猜节点。
- 改完一轮再 `validate`，不要每个 op 都跑（文件几百 KB，逐次读写没必要）。

## 3. 工具 × CLI 对照

MCP 工具名 = `paintress_<命令>`，参数与 CLI 同名；CLI 适合在 bash 里一次跑完、也适合脚本。

| 用途 | MCP | CLI |
| --- | --- | --- |
| 新建 `.ptr` | `paintress_new(path, name?, width?, height?, fill?, layout?, padding?, gap?, overwrite?)` | `paintress new <file> [--name n] [--width n] [--height n] …` |
| 文档摘要 | `paintress_doc_info(path)` | `paintress info <file>` |
| 查节点 | `paintress_query(path, type?, name?, nameRegex?, id?, reusable?)` | `paintress query <file> [--type t] [--name n｜--regex r]` |
| 取子树 | `paintress_get(path, id, depth?)` | `paintress get <file> <id> [--depth n]` |
| 改属性（浅合并） | `paintress_update(path, id, patch)` | `paintress update <file> <id> '<json>'` |
| 插节点 | `paintress_insert(path, parent, node, index?)`（`parent="root"` 插顶层；`node` 缺 id 自动生成） | `paintress insert <file> <parent> '<json>' [--index n]` |
| 删节点（含子树） | `paintress_delete(path, id)` | `paintress delete <file> <id>` |
| 移动 / 重排 | `paintress_move(path, id, parent, index?)` | `paintress move <file> <id> <parent> [--index n]` |
| 批量原子 | `paintress_batch(path, ops)` | `paintress batch <file> '<ops-json>'` |
| 校验 | `paintress_validate(path)` | `paintress validate <file>` |
| 渲染 HTML | `paintress_render(path, out)` | `paintress render <file> <out.html>` |
| 截图 PNG | `paintress_screenshot(path, out, scale?, board?)` | `paintress shot <file> <out.png> [--scale n] [--board id｜name]` |
| 读人的点击标记 | `paintress_marks(project?, action?)` | — |
| viewer | — | `paintress web [--port n] [--daemon]` / `ping` / `status` / `stop` |
| 注册目录给人看 | — | `paintress watch <name> <dir> [--depth n]` / `unwatch <name>` / `attach <name>` |

`update` 的 `patch` 是**浅合并**：只写要改的键；`id` 与 `type` 不可改（要换类型就删了重插）。

## 4. 节点 schema 速查（pen flavor 2.21）

顶层：`{ version, fileToken, themes, variables, children[] }`
（paintress 重排后的顺序固定为 version → fileToken → themes → variables → children）。

节点类型（`validate` 认得的）：`frame` / `group` / `rectangle` / `ellipse` / `text` / `icon` /
`image` / `path` / `polygon` / `ref`。写错类型不会报 error —— 只给一条「未知类型，已原样保留」的 warning，
所以 `validate` 的 warnings 要看。

| 关注点 | 写法 |
| --- | --- |
| 坐标 / 尺寸 | `x` `y` `width` `height`；`width: "fill_container"` 撑满父级（`height` 同理可用 `fit_content`） |
| 自动布局 | `layout: "vertical"｜"horizontal"｜"none"` + `gap` + `padding`（数字或 `[上,右,下,左]`）+ `alignItems` + `justifyContent`。**`frame` 不写 `layout` 默认是 `horizontal`**，要靠 `x`/`y` 定位就把父级设 `"none"` |
| 背景 / 描边 | `fill`（色值或 `"$变量名"`）、`cornerRadius`、`stroke` + `strokeWidth` + `strokeAlignment`、`opacity`、`clip`、`rotation` |
| 文字 | **`content`**（不是 `text`，写错就渲染成空）+ `fontFamily` / `fontSize` / `fontWeight` / `textAlign` / `lineHeight` / `textGrowth` / `fill` |
| 图标 | `icon: "<lucide 名>"` + `library: "lucide"`（`feather` 也可渲染） |
| 图片 | `image` 节点**渲染不出真照片**（占位），画稿一律用色块占位（与 Pencil 时代结论一致） |
| 主题 / 密度 | board 上 `theme: { mode, density }`；`themes` 声明轴与取值 |
| 变量 | `variables.<名> = { type: "color", value: [ { value: "#RRGGBB", theme: { mode: "dark" } }, … ] }`；节点里写 `"$surface-bar"` |

- 变量名 = CSS 变量名（去掉 `--`）= `tokens.css` 里的令牌名，见 `.agents/memory/DESIGN.md` §9。
- `ref` 是复用节点（组件实例）；现有稿里用得少，批量改造前先 `get` 看清结构再动。

## 5. 新建 / 拆分设计稿（现在 Agent 自己就能做）

> **扩展名口径（2026-11-12 崔总定）**：**新建文件一律 `.ptr`**；`.pen` 只作兼容（既有四份继续维护，不主动迁移）。
> 依据：上游 README 写明「`.pen`，以及它自己写出的 `.ptr`」「`.pen` 层冻结，`.ptr` 层演进」。
> 注：**paintress 的 MCP server 不提供 instructions**（实测 “does not provide instructions”），格式口径只能从它的 README / `validate` 拿；不存在类似 Pencil 那种「服务端谎称文件加密」的信息。

**新建（0.4.0 起走工具，不要手写）**：`paintress_new`（MCP）或 `paintress new`（CLI）写出一份合法 `.ptr` ——
带 `x-paintress` 块、一个空 board，返回 `boardId`。不会覆盖既有文件（除非显式 `overwrite`），给不带扩展名的路径会自动补 `.ptr`。

```bash
paintress new design/<模块>-<视图>.ptr --name <名字> --width 1440 --height 900
paintress validate design/<模块>-<视图>.ptr     # 期望 errors 0
paintress info design/<模块>-<视图>.ptr         # 核对顶层 board 与变量
```

要**引用令牌**就在同一批改动里用 `paintress_update` 往文件写 `variables`；确实需要一次性铺好大结构时，
仍可手写 JSON（**最小只要 `version` + `children`**、每个节点带唯一 `id`）——但先用 `new` 起步更省事。

**拆分现有大稿**（`main.pen` 里塞了三套外壳，`design/main.md` §1.1 记的拆分方案）：
用 `paintress_get` 取整块 board → `insert root` 进新文件（新文件用 `.ptr`）→ `validate`；
或复制整个文件后 `delete` 掉不要的 board（保留 `variables` / `themes`，颜色引用才不会断）。

## 6. 视觉自检：screenshot 是 Agent 的眼睛

- `paintress_screenshot` 需要本机 Chrome/Chromium（本机 `/usr/bin/google-chrome`）；
  不传 `board` 时输出**整张画布**（所有 board 拼在一起），`scale` 默认 2。
- **单帧截图直接传 `board`**（0.5.1 起）：id 精确 → name 精确 → 大小写不敏感的模糊匹配；
  命中多个会**报错并列出候选**（不猜），画板尺寸为 0 时报错。返回里带 `board: {id,name,x,y,width,height}`。

```bash
paintress shot design/main.pen /tmp/one-board.png --board rJRpX --scale 1
paintress shot design/main.pen /tmp/detail.png --board "Dialog / 关于 About" --scale 1
```

- 老办法（0.5.0 以前没有 `board` 时的兜底，现在一般不需要）：把一块 board 抽成临时文件、`x`/`y` 归零，否则画布会按原绝对坐标留出大片空白：

```bash
python3 - <<'PY'
import json
d = json.load(open('design/browse.pen'))
board = next(n for n in d['children'] if n['id'] == 'o2OJy2')
board = json.loads(json.dumps(board)); board['x'] = board['y'] = 0
json.dump({k: d[k] for k in ('version','fileToken','themes','variables')} | {'children': [board]},
          open('/tmp/one-board.pen', 'w'), ensure_ascii=False)
PY
paintress shot /tmp/one-board.pen /tmp/one-board.png --scale 1
```

- 已知数据问题：`main.pen` 的 `Dialog / 关于 About`（`FPU9M`）高度为 0（480×0），单画板截图会直接报错 —— 修稿时顺手补高度。

- 渲染告警要读：图标库只支持 `lucide` / `feather`，现有稿里 `Material Symbols Rounded` 的图标
  （browse 32 / editor 6 / export 12 / main 24 个）会渲染成**占位框**；新画的图标一律用 `lucide` 名。
- `render` 出的 HTML 是给**人**看的（每节点带 `data-node-id`）；自己复核用截图。

## 7. 人机协作：viewer + 短号

```bash
paintress web --daemon --idle 3600000   # http://localhost:3316（本机已有 daemon 在跑）
paintress watch <项目名> <目录>          # 注册一个目录供浏览
# 打开 http://localhost:3316/?project=<项目名>&file=<文件名>
```

人在浏览器里点节点 → 该节点拿到 `#1 #2 …`，Agent 用 `paintress_marks` 读回映射。
`paintress status` 看 daemon 与已注册目录（含 `version`）；用完不必停（它按 `--idle` 自己退）。

**项目解绑（0.5.x 起的自动行为）**：自动注册（`owner: agent`）的项目在 agent 连接全断、没浏览器在看时，
**宽限 45 秒**后自己从 registry 里退场（跨过 agent 崩溃重启的窗口；期间重新 `attach` / 打开浏览器都会撤销）。
手动 `paintress watch` 绑的（`owner: manual`）**不会**自动退，只能人在查看器里点项目行的 `×` 解绑或 `paintress unwatch`。
→ 所以「昨天还能打开的项目今天不见了」是预期行为，重新开会话 / 重新 `watch` 就回来。

查看器侧栏 0.5.x 还有两个入口：项目行悬停的 **`×` 解绑**、底部的**关闭服务器**（带连接数 + pid 二次确认，
通知的是 `paintress web` 重启）；徽章在 npm 有新版本时会变成上下两层并弹出可复制的升级提示。

> ⚠️ `paintress watch <name> <dir>` 的**相对路径按 daemon 的 cwd 解析**（不是你的 shell cwd；本机 daemon 跑在 `/home/andares`）——
> 用**绝对路径**最稳。本仓已被注册为项目 `c-thun-raybend`（`/home/andares/repos/c-thun/raybend`，depth 3），
> 直接开 `http://localhost:3316/?project=c-thun-raybend&file=design/main.pen` 即可。

## 8. 纪律与坑

1. **先备份再批量改**：`cp design/x.pen /tmp/x.pen.bak-$(date +%s)`。
2. **单写者**：paintress 直接写磁盘。改之前确认没人同时在别的编辑器里开同一文件并保存
   （旧 buffer 保存 = 整体覆盖）。不要并发跑两个改同一文件的 paintress 进程。
3. **首次写入会重排整个文件**（键序变成 version/fileToken/themes/variables/children，缩进 2 空格，
   语义不变）。所以第一次用 paintress 改某份 `.pen` 会出现上千行 `git diff` —— 这是**格式 diff**，
   不是内容变化；submit 时要么单独提交这一次格式整理，要么在实施记录里说明。
4. **`validate` 的 warnings 不是噪音**：未知节点类型、重复 id、未定义变量都只报 warning，
   但都会让渲染出错（空白 / 占位框）。它**不校验属性取值**：`alignItems: "middle"` 这类写错也 0 error 0 warning，
   只是静默不生效 —— 结构与关键状态改完必须**看一眼截图**。
5. **改错类型不可改**：`id` / `type` 不可变，要换类型就 `delete` + `insert`。
6. **结构 / 状态 / 交互以画稿为准，纯尺寸以实现为准**（`.agents/memory/DESIGN.md` §8.4）：
   画稿尺寸是硬写的字面量，尺寸令牌的真相源是 `src/styles/tokens.css`。
7. **截图 ≠ 验收**：截图是 Agent 自检。真实 DPI、字体渲染、照片观感仍由崔总在真机看
   （`AGENTS.md` §2.8 的测试分工不变）。
8. **`.pen` 是明文 JSON**：可以用 `python3` / `jq` 做只读统计与临时抽帧；
   结构化改动走 paintress（有校验 + 原子 batch），别手写大段 JSON 再祈祷没写错。
