# 官网（`website/`）与本体分家：根 AGENTS.md 加说明 + 新建 `website/AGENTS.md`

完成时间：2026-09-17 13:17:53 CST

本次**只动文档**，不碰任何代码与配置。目的：把「`website/` 是官网、技术栈与 raybend 本体不同」这件事写进 AGENTS.md，
避免后续（人或 Agent）把两套选型混起来。

---

## 1. 改动范围

| 文件 | 改动 |
| --- | --- |
| `AGENTS.md`（根） | ① §3 版本基线加一句限定：**下表只描述软件本体**；② §4 目录树加 `website/` 一行；③ §4 目录树后新增小节「**`website/` 是官网，不是应用本体（不要弄错）**」——本体 vs 官网对照表 + 依赖独立 + 纪律适用范围 + 产物与发布通道；④ §10 文档索引加 `website/AGENTS.md` 一行 |
| `website/AGENTS.md` | **重写**（原文件只有 Solid 2 反应式诊断那一段，来自模板）。新版本：定位（是官网、不是本体）+ 技术选型表（逐项带实测版本）+ 目录与约定 + 命令 + 边界与硬规矩 + 现状与待办 + 保留并扩写原反应式纪律 |

## 2. 关键事实（写文档前逐条核过，不是抄描述）

- **官网的选型**（读 `website/package.json` 与 `website/node_modules/*/package.json` 的**实测版本**，非范围）：
  `solid-js` / `@solidjs/web` `2.0.0-rc.6`、`@solidjs/router` `2.0.0-next.21`、`@solidjs/meta` `1.0.0-next.2`、
  `@solidjs/vite-plugin` `3.0.0-next.38`（`start: true`）、`filesystem-routing` `0.2.1`、
  `tailwindcss` / `@tailwindcss/vite` `4.3.3`、vite `8.2.2`、typescript `5.9.3`、vitest `5.0.0`、
  oxlint `1.79.0` + `eslint-plugin-solid` `0.18.0`（v2 规则集）、`@solidjs/diagnostics` `2.0.0-rc.6`。
- **它不是 `@solidjs/start` 单体包**，而是 SolidStart 2.0 拆出来的那套库（vite-plugin 的 start 模式 + filesystem-routing + router 2.x）。
  文档里写清了这一点，免得后来者去找不存在的 `@solidjs/start` 依赖。
- **依赖完全独立**：`website/` 自带 `package.json` / `pnpm-lock.yaml` / `pnpm-workspace.yaml` / `node_modules`；
  根目录没有 `pnpm-workspace.yaml` —— 两边不构成 pnpm workspace 关系，根目录 `pnpm install` 不会装官网依赖。
- **Lucide 尚未安装**：`npm view lucide-solid peerDependencies` → `{ 'solid-js': '^1.4.7' }`，
  即最新 `lucide-solid@1.46.0` 的 peer **还没有 Solid 2.0**。故文档里标为「已定但未装 + 兼容性未验证」并写了退路，
  没有假装它已经在跑。（`@lucide/solid` 这个名字在 npm 上不存在，别用错。）
- **GitHub Actions 尚未落地**：仓库里没有 `.github/`，Pages 工作流还没写。文档按「计划 + 两个坑（子路径 base、SPA 深链接 404）」记，
  没写成已完成的事实。

## 3. 顺手记下的两个隐患（已写进 `website/AGENTS.md` §5，未动手修）

1. **`public/*.webp:Zone.Identifier` 是 NTFS 附加流残留**，文件名里真带冒号。这种文件名在 Windows 上**无法 checkout**，
   一旦提交会脏掉 Windows 侧（本项目 Windows 优先）。根 `.gitignore` 里没有对应规则。
2. 模板演示内容（`src/routes/index.tsx` 的 Hello Solid、`users`、`Counter`、`public/users.json`）还在。

## 4. 验证方式

纯文档改动，无构建/测试可跑。做的是**事实核对**：

- `git diff package.json` —— 确认根 `package.json` 的改动（`spike:win`）与本次无关；
- `git status --short` —— `website/` 目前仍是**未跟踪**目录（本次未提交，也未 `git add`）；
- `node -p "require('./node_modules/<pkg>/package.json').version"` —— 逐项取官网的真实安装版本；
- `npm view lucide-solid peerDependencies` —— 拿 Lucide 的 peer 证据。

## 5. 遗留问题

- **GitHub Pages 的 Actions 工作流**没写（本次范围外，用户明确「一开始仅做这些」）。
- **`website/` 仍是未跟踪状态**：要不要连同模板一起提交、哪些模板残留要先删，待用户定。
- 官网的**语言切换方案**未定型（`public/` 已备中英两套素材），文档里标为待补。
