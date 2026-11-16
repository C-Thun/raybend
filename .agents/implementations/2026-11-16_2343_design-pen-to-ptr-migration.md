# 2026-11-16T23:43+08:00 — 设计稿四份主稿从 .pen 迁移到 .ptr，并同步路径引用与口径

1. **四份主稿 `.pen` → `.ptr`**：用 MCP `paintress_convert` 逐份转换（CLI 0.8.1 没有 `convert` 子命令），源文件随后 `git rm`。
   - `main.ptr` 2231 节点、`browse.ptr` 4269、`editor.ptr` 5842→5685（实测 5685）、`export.ptr` 2835，与转换前计数一致。
   - `paintress validate` 四份全 `ok=true, errors=0`（各 1 条既有 warning：`Inter` 字体未登记，转换前就有）。
2. **等价性验收（两层）**：
   - 语义：去掉 `fileToken` / `x-paintress` 后规范化 JSON 的 sha256，四对**完全一致**；
   - 渲染：转换前后各截一张图，`compare -metric AE` 为 `browse 0 / editor 0 / export 0`，`main 84`（1.4×10⁷ 像素中的字体回退抖动）。
3. **引用同步**：把「指向当前设计稿」的 `design/x.pen` 路径改成 `.ptr`（53 个文件，含 AGENTS.md / PAINTRESS.md / DESIGN.md / COMPONENTS.md / FUNCTION-BROWSE.md / FINISHED.md / FUTURE.md / docs / specs / design 同名文档 / 4 个 src 文件注释）；`.agents/implementations/**` 按「历史不回改」纪律保留原样。
4. **口径更新**：`AGENTS.md` §2.6/§4/§5.1/§10/§12、`PAINTRESS.md` §1/§5 由「新建用 .ptr，既有 .pen 兼容维护」改为「一律 .ptr；.pen 只在 git 历史里」，并注明历史记录里的 `design/x.pen` 指的就是今天这些 `.ptr`。
5. **补手册**：`PAINTRESS.md` 新增 `paintress_convert` 用法段落（MCP-only、新 fileToken、默认不覆盖）与可复用的验收脚本；`FINISHED.md` 的硬规矩由「Pencil 落稿」改为「paintress 落稿」。
6. **`design/__preset.ptr` / `__components.ptr`**：`paintress_viewer` 自动建立的 0.8.x 设计系统文件（token 组与组件库目前为空），一并入库，与 AGENTS.md 的新表述对上。

## 不在仓库内、但同一轮做的

- 退役 Pencil 的全局注册：`~/.pi/agent/mcp-adapter.json`（删 `pencil` 条目）、`~/.claude.json`（删 `mcpServers.pencil`）、`~/.pi/agent/mcp-cache.json`（清缓存）、`~/.agents/skills/pencil-design/`（移到 `~/.agents/retired-skills/` 并从 `.skill-lock.json` 摘除）。四处都留了 `*bak-pencil-<时间戳>` 备份。本会话内存里仍显示 pencil，下次启动 pi 才会消失。

## 验证

- `paintress validate design/*.ptr`：四份 errors 0；`paintress validate design/__*.ptr`：0 error / 0 warning。
- 转换前后截图对比见上；`paintress_clear_changes` 已调用（本轮回传 `cleared: 0`，说明没有残留高亮）。
- `pnpm typecheck`（根，Solid 1 应用）通过：4 个 src 文件只改注释里的路径。

## 遗留

- `validate` 的 `Inter` 字体未登记 warning（既有）：需要 `paintress_font add` 才能消掉，属设计系统配置，未在本轮擅自加。
- `.agents/implementations/**` 与 `FINISHED.md` 的里程碑段落里仍有 Pencil 时代叙述（`design/main.md` §6.1 等），按纪律留作历史；`PAINTRESS.md` 已说明映射关系。
- 转换后 `fileToken` 变新，查看器里针对旧 `.pen` 的书签需要重取。
