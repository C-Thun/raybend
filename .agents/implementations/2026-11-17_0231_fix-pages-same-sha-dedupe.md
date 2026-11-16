# 2026-11-17T02:31+08:00 — 修官网部署被 Pages 按 commit SHA 去重：发版提交的 push 不再抢部署名额

v0.2.0 发版后官网一直显示 0.1.4 的下载链接，Actions 却全绿。查清了根因并修好。

## 根因（有日志证据）

GitHub Pages 按 `pages_build_version`（部署时的 commit SHA）去重，**同一 SHA 只认第一次部署**，artifact 内容不同也不例外。发版时「推送发版提交」和「发布 release」是同一个 commit：

| 时间 (UTC) | 事件 | artifact 里的版本 | `pages_build_version` | 结果 |
| --- | --- | --- | --- | --- |
| 17:43 | push（master） | v0.1.4 | `c6f9eff6…` | **占住该 SHA** |
| 17:47 | release（v0.2.0） | **v0.2.0** | `c6f9eff6…` | 被去重，跳过 |
| 18:25 | 重跑 deploy | v0.2.0 | `c6f9eff6…` | 仍被去重 |

线上 `index.html` 的 `last-modified` 是 17:43:32，新构建独有的 `virtual_solid-ssr-entry-client-DkYpabP0.js` 直接 404（origin 没有它）—— 说明不是 CDN 缓存，是 Pages 根本没换产物。DNS 也确认没有 Cloudflare 之类的中间层（CNAME → `c-thun.github.io`）。

上几版没踩到，是因为 0.1.4 那次发版提交没有改动 `website/**`，push 不会触发站点构建，release 事件成了该 SHA 的唯一部署方。

## 修法

`.github/workflows/website.yml` 的 `build` job 加条件：**commit message 含「发布版本」的 push 不构建**（`contains(github.event.head_commit.message, '发布版本')`），把该 SHA 的唯一部署权让给 release 事件。用 `contains` 而不是 `startsWith('chore: 发布版本')`，是因为表达式里出现 `: ` 会把 YAML 顶成映射（第一次写就踩了，改成单行折叠块 + 无冒号匹配才解析通过）。

配套文档：
- `website/AGENTS.md` §5 加「同一个 commit 只能部署一次」的说明与本次实测；
- `docs/release.md` §3.2 加同一条纪律：**发版后打开官网确认版本号，别只看 Actions 结论**；
- `scripts/lib/release-upload.mjs` 的「更新清单不一致」报错补上可操作提示（旧目录需重新构建，不要手改 `latest.json`）。

## 验证

- YAML 解析通过，`jobs.build.if` 是**单行**（折叠块缩进与首行对齐，`>-` 才会折成空格；多缩进会保留换行）且 `runs-on` 正常；
- `pnpm test:release` 112 项全绿；`node --check` 通过；
- markdownlint 报的 `MD056`（L278 `--ai=auto|required|off` 的竖线）与 `MD060` 是**既有**问题，已确认不在本次 diff 内，按范围不处理。

## 需要人类做的一步

本次修复的提交本身触碰了 `website/**` 之外的 workflow 文件（在 push 忽略名单内），所以**推送它不会触发站点构建**；线上仍要等一次新的 website 相关推送。想让 0.2.0 立刻上线，最省事的是：`gh workflow run website.yml --ref master -f tag=v0.2.0` 之后**再推一个小改动**——不行，同 SHA 仍被去重；正确做法是下次任何触碰 `website/**` 的推送（新 SHA），届时注入的版本就是 `releases/latest` = v0.2.0。
