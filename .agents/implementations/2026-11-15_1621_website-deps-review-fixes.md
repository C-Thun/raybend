# 2026-11-15T16:21+08:00 — 官网依赖升级复审：固化浏览器冒烟并消除高危间接依赖

本轮只处理第三方复审确认属于官网依赖升级的问题：

- 原升级验证使用 `/tmp` 下的一次性 CDP 脚本且没有断言，不满足可重复 E2E 的项目纪律。新增 `scripts/smoke-ui.mjs` 和 `pnpm smoke:ui`，零新增运行时/开发依赖，使用 Node WebSocket 与本机 Chrome，自动断言首页七段、样式/字体/图片加载、语言切换与刷新持久化、404、console/运行时异常和资源失败；通过 `frameNavigated` 处理 dev 下 reload 竞态。`website/AGENTS.md` 已补命令说明。
- `pnpm audit` 发现 6 个 high，定位到 `brace-expansion` 5.0.9、`source-map-js` 1.2.1、`follow-redirects` 1.16.0。按现有 semver 范围更新 lockfile 至 5.0.13 / 1.2.2 / 1.16.3，没有新增 override。重跑 icons 与品牌字生成，所有生成物逐字节不变。
- 直接解包比较 Noto Sans SC 5.3.0 / 5.3.1 的 101 个共用 WOFF2 子集，在 100/400/500/600/700/900 六档权重实例化后逐字比较 hmtx，606 组字体/权重组合排版宽度差异为 0。

验证：`pnpm install --frozen-lockfile`、`pnpm typecheck`、Vitest 28/28、`pnpm lint`（仍只有既有 Hero structured-class 警告）、`node --check scripts/smoke-ui.mjs`、对该脚本的 oxlint、`pnpm build`、生产预览及 dev server 的 `pnpm smoke:ui` 均通过。审计中的 6 个 high 已消除；剩余 2 个 moderate 为开发期 ESLint/minimatch（需上游接受 minimatch 11）与 potrace/Jimp/phin（暂无修复版），未强行覆盖版本。

范围外记录：`RAYBEND_BASE=/raybend/` 子路径构建访问空白，使用 `git archive HEAD` 独立安装升级前依赖也复现，确认是既有问题而非本次回归；未在本轮修改，定位与建议已写入前一实施记录的复审段落。
