# paintress 0.2.3 → 0.4.0 升级

时间：2026-11-12 21:11:53

## 改动范围

系统全局工具 paintress 升级，以及随之同步的 agent skill 与本仓操作手册。

| 对象 | 动作 |
| --- | --- |
| 全局 npm 包 | `npm install -g paintress@latest`：`0.2.3` → **`0.4.0`**（changed 100 packages in 24s） |
| `~/.pi/agent/skills/paintress/SKILL.md` | 用包内 `skills/paintress/SKILL.md` 覆盖（旧版与 0.4.0 内容差异较大，旧文件备份在 `/tmp/paintress-skill-bak-20261112-211030.md`） |
| `.agents/memory/PAINTRESS.md` | 版本号、新建方式、工具表同步（见下） |

## 0.4.0 的关键变化（影响到本仓用法的部分）

- **新增 `paintress_new` / `paintress new`**：直接产出一份合法 `.ptr`（带 `x-paintress` 块 + 一个空 board，返回 `boardId`），
  拒绝覆盖既有文件（需显式 `overwrite`），不带扩展名的路径自动补 `.ptr`。
  → 以前「手写最小 JSON 起步」的做法降级为「要一次性铺大结构时」才用。
- 格式口径不变：`.ptr` 是标准格式且持续演进，`.pen` 只作兼容（读/写/渲染一致，不悄悄转换）。
- skill 内容变化：补了「`type` / `id` 不可改、文字在 `content` 而非 `text`、`update` 是浅合并不校验字段名」、
  「查看器 / 目录注册由 agent 自己 `paintress watch`，别让用户手动跑」等条目。
- MCP server 仍**不提供 instructions**（复核：`does not provide instructions`），格式口径照旧从 README / `validate` 拿。

## 同步的文档改动（`.agents/memory/PAINTRESS.md`）

1. §开头「依据」行：保留 0.2.3 在四份 `.pen` 上的实测结论，另加 0.4.0 复核范围。
2. §1 版本号改 `0.4.0`，补升级命令与 skill 同步步骤。
3. §1.1 对照表「Agent 无法新建」一行：改为 `paintress_new` / `paintress new` 为第一选择。
4. §3 工具 × CLI 对照表：新增「新建 `.ptr`」一行。
5. §5 新建：改为走工具起步（`new` → `validate` → `info`），手写 JSON 退为可选项。

## 验证

- CLI 冒烟（`/tmp/pt-check`，临时目录，已清理）：`new` / `info` / `validate`（errors 0 / warnings 0）/ `render` / `shot`（命中 `/usr/bin/google-chrome`）全通。
- MCP：重连后列到 **13 个工具**（含新增 `paintress_new`），并用 `paintress_new` 实际建出 `mcp-smoke.ptr`。
- viewer daemon：`paintress status` 报 `version: 0.4.0`，项目 `c-thun-raybend` 在册（depth 3、4 个文件），`http://localhost:3316/` 返回 200。
- `paintress doctor`：**pi** 的 MCP 条目 `noop`（已最新）。

## 遗留问题（等崔总决定）

`paintress doctor` 显示另外三个客户端的条目还没写（本次未动它们）：

- `~/.config/Code/User/mcp.json`（VS Code）— 需新增条目
- `~/.codex/config.toml`（Codex）— 追加 `[mcp_servers.paintress]`
- `~/.config/opencode/opencode.json`（OpenCode）— 需新增条目

要补的话一条命令：`paintress install vscode codex opencode`（会先写 `*.bak-<时间戳>` 备份，只合并 paintress 一条）。
Claude Desktop / Cursor / Windsurf 本机没装，无需理会。

## 未验证项

本次只做到 CLI / MCP / daemon 的接口级复核；**viewer 页面的实际观感（0.4.0 的 UI 变化）未经人眼确认**，需要崔总在浏览器里看一眼。
