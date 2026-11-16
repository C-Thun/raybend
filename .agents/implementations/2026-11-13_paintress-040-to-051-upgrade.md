# paintress 0.4.0 → 0.5.1 升级

时间：2026-11-13 03:59:50

> 前一次升级（0.2.3 → 0.4.0）：`.agents/implementations/2026-11-12_paintress-034-to-040-upgrade.md`。

## 改动范围

| 对象 | 动作 |
| --- | --- |
| 全局 npm 包 | `npm install -g paintress@latest`：`0.4.0` → **`0.5.1`**（changed 100 packages in 6s） |
| `~/.pi/agent/skills/paintress/SKILL.md` | 用包内 0.5.1 的 `skills/paintress/SKILL.md` 覆盖（旧文件备份 `/tmp/paintress-skill-bak-20261113-035841-v040.md`） |
| viewer daemon | `paintress stop` → 重新 `web --daemon --idle 3600000`，现跑 **0.5.1** |
| `.agents/memory/PAINTRESS.md` | 版本号、§3 截图工具签名、§6 单帧截图流程、§7 项目解绑/查看器新入口 |

## 0.5.x 的变化（依据上游 `.agents/implementations/` 记录 + 本机实测）

发布节奏：0.5.0（2026-11-12 18:42）、0.5.1（19:54 UTC 表内；本机时间 2026-11-13 凌晨）。

1. **画板级截图** `paintress_screenshot(..., board?)` / `paintress shot --board <id|name>`：
   匹配顺序 id 精确 → name 精确 → 大小写不敏感模糊；多命中**报歧义并列候选**；尺寸为 0 报错；
   返回带 `board: {id,name,x,y,width,height}`。
   → **替掉了我们 §6 用 python 抽单帧的老办法**（保留为兜底）。
2. **查看器项目解绑线**：自动注册（`owner: agent`）的项目在 agent 全断、无浏览器时宽限 **45s** 后自动退场；
   手动 `watch` 的不会自动退；查看器项目行悬停出 `×` 手动解绑（二次确认，需 `x-paintress: 1` 头）。
3. **查看器可关停 web 进程**：侧栏底部「关闭服务器」按钮，二次确认里列出连接数与 pid。
4. **查看器版本升级提示**：npm 有新版本时徽章变上下双层（锈红下一层），点击弹可复制的升级指令；失败/离线静默。
5. **变更高亮柔化**：仍是 2px 描边，脉冲改 2s 无限透明度循环（25%↔100%），并尊重 `prefers-reduced-motion`。

## 验证

- MCP 重连：13 个工具，`paintress_screenshot(path, out, scale?, board?)` 已带新参数。
- **单画板截图实测（MCP）**：`design/main.pen` + `board: "rJRpX"` → 1600×1000（整张画布是 19360×4036），返回 `board` 元数据；
  截图目视确认只含该 board、内容正常。
- **单画板截图实测（CLI）**：`design/browse.pen --board o2OJy2` → `Shell / Browse / Tiles` 1600×1000。
- **0 尺寸画板**：`main.pen` 的 `Dialog / 关于 About`（`FPU9M`）报「尺寸为 0（480×0），无法截图」—— 报错清晰，
  同时暴露一处**画稿数据问题**（该 board 高度为 0），已记进 `.agents/memory/PAINTRESS.md` §6。
- daemon：`paintress status` → `version: 0.5.1`，项目 `c-thun-raybend` 在册，`/api/projects` 返回 `version: 0.5.1`、`latest: null`（未探测到新版）。
- `http://localhost:3316/` → 200。
- `paintress doctor`：pi 条目仍 `noop`。

## 遗留问题

1. `paintress doctor` 仍显示 VS Code / Codex / OpenCode 没有 paintress 条目（与上次相同，等崔总决定是否 `paintress install vscode codex opencode`）。
2. `design/main.pen` 的 `Dialog / 关于 About`（`FPU9M`）高度为 0，是**画稿数据缺陷**（不是 paintress 问题），下次动那份稿时补。
3. 查看器 0.5.1 的新 UI（解绑 `×`、关闭服务器、升级徽章）**未经人眼验收**；我只做了接口级与截图级复核。
