# 官网国内镜像（rb.cthun.com）

> 状态：**方案待拍板，未开工**（2026-10-24 从 `memory/FUTURE.md` 的 Future Release #6 移入「发版后」）
> 目标：给大陆访问一条不经过 GitHub Pages 的官网入口；页面优先，安装包是否镜像另定。
> 相关：`memory/FUTURE.md` §「发版后」；`website/AGENTS.md` §5；`website/src/data/release.ts`。

## 1. 背景

崔总 2026-09-30 提出：国内访问官网慢；可以另找免费静态托管，另开 `rb.cthun.com` 国内专供。
当时查证后排除了 Gitee Pages（2024-05 下线）与 Deno Deploy（无大陆节点），选定腾讯 EdgeOne Pages（现 EdgeOne Makers）。
`cthun.com` 已在腾讯云个人账号完成实名与 ICP 备案，`rb.cthun.com` 子域共享备案，CNAME 接入即可。

**非目标**：不另建一套官网源码/文案；不改主站 GitHub Pages 链路；安装包 CDN 需按 §4.3 单独拍板。

## 2. 现状（2026-10-24 核对）

- 主站：`https://raybend.cthun.com/`，GitHub Pages；`rb.cthun.com` 尚无解析（NXDOMAIN）。
- 发布：`.github/workflows/website.yml` 在 push `website/**`、`release: published`、手动触发时构建并部署 Pages。
- 取版：构建期注入（`website/vite.config.ts` + `website/scripts/release-info.ts`），不是运行时 XHR。
  - `RAYBEND_TAG` 优先；否则 `CI=true` 或 `RAYBEND_FETCH_RELEASE=1` 时查 GitHub `releases/latest`；否则 pending。
  - 当前没有网络失败的静态兜底：请求失败会让构建失败。
- 下载：`website/src/data/release.ts::pickInstaller` 只接受 `github.com/C-Thun/raybend/releases/download/` 的资产 URL，优先 MSI。
- 最新正式版 `v0.1.3` 资产：MSI 约 120.5 MB（无 NSIS）。本地产物 `v0.1.1` 里的 NSIS 约 15.5 MB。
- 产物：`website/dist/client` 约 6 MB / 122 个文件；最大文件在 25 MB 以内。

## 3. 选型与硬前提

- **EdgeOne Makers（原 EdgeOne Pages）** https://pages.edgeone.ai/ ：免费静态托管 + 腾讯 CDN 大陆节点 + 自定义域名 + 自动 SSL。
- 排除：
  - Gitee Pages：2024-05 下线，https://www.landian.news/archives/103754.html 。
  - Deno Deploy：无大陆节点，https://docs.deno.org.cn/deploy/manual/regions/ 。
- 备案：`cthun.com` 已实名 + 已 ICP 备案；`rb.cthun.com` 子域共享备案。加速区域选「中国大陆可用区」或「全球可用区（含中国大陆）」时，自定义域名需备案——已满足。
- 免费版配额（2026-10-24 官方文档）：项目 40 个；构建 500 次/月、并发 1、单次 20 min、4C6G；单文件 25 MB；单项目 2 万文件；总存储 5 GB；Blob 1 GB / 单值 25 MB；自定义域名 200 个；免费 SSL。
- Node：预装 24.18.0 等；pnpm 文档列 6–11（`website` 锁 12.3.4，需额外处理）。
- 大陆网络访问「项目域名 / 部署域名」只有 3 小时预览链接；**必须绑自定义域名**才有稳定入口。

## 4. 方案

### 4.1 推荐：GitHub Actions 构建一次，双发 Pages + EdgeOne

理由：主站已有完整的构建与取版逻辑；镜像复用同一份 `dist/client`，版本/内容天然一致，不受境内构建访问 GitHub API 与 pnpm 版本影响。

步骤：

1. EdgeOne 控制台开通 Makers，创建**直接上传**类型项目（生产环境），加速区域选中国大陆。
2. 生成 API Token，存 GitHub 仓库 secret `EDGEONE_API_TOKEN`。
3. `.github/workflows/website.yml` 在 `pnpm build` 后加一步：
   `npx edgeone makers deploy ./website/dist/client -n <project> -t "$EDGEONE_API_TOKEN" -e production --site china`
4. 首次部署后，在 EdgeOne 控制台给项目绑 `rb.cthun.com`，按提示到腾讯云 DNS 加 CNAME；等 SSL 签发。
5. 验证：`https://rb.cthun.com/` 打开；版本号/下载按钮与 `raybend.cthun.com` 一致；release 后自动更新。

注意：CLI 部署要求项目为「直接上传」类型；`--site china` 选中国站；项目名与自定义域名只配一次。
如果不想在 workflow 里装 CLI，也可以在 release workflow 里 POST EdgeOne 部署钩子，让 EdgeOne 自己拉 Git 构建——但会回到 §4.2 的构建环境问题。

### 4.2 备选：EdgeOne Git 集成 + 部署钩子

1. EdgeOne 创建 Git 集成项目，关联 `C-Thun/raybend`，生产分支 `master`。
2. 根目录 `website`；安装 `pnpm install --frozen-lockfile`；构建 `pnpm build`；输出 `dist/client`；Node 24。
3. 处理 pnpm 12.3.4：用 corepack/npx 指定版本，或在项目设置里用兼容命令；需实测。
4. 构建环境设 `RAYBEND_FETCH_RELEASE=1`（或确认 EdgeOne 注入 `CI=true`），保证取版；若访问 `api.github.com` 不稳定，会构建失败。
5. 在 `.github/workflows/website.yml` 的 release 路径加一步 POST EdgeOne 部署钩子，解决「只在 push 时自动部署」的缺口。
6. 加 `website/edgeone.json` 固化 build/install/output，避免控制台配置漂移。

### 4.3 下载策略（待拍板）

| 方案 | 优点 | 代价 |
| --- | --- | --- |
| 仍走 GitHub Releases | 零成本、零改动 | 大陆下载慢，镜像只解决页面 |
| ghproxy 类前缀 | 零成本 | 公共代理稳定性自担；需改 `pickInstaller` / 下载 URL 逻辑 + 测试 |
| 只镜像 NSIS（约 15.5 MB） | 可放进 EdgeOne 25 MB 限制 | 当前 latest 没有 NSIS；需发版时同时产 NSIS；镜像与主站资产偏好不一致 |
| 腾讯 COS + CDN | 可放 120 MB MSI，国内直下 | 有存储/流量费用，不再是纯免费 |

当前结论：**页面镜像免费可行；安装包国内直下不在 EdgeOne Makers 免费版能力内**。建议先做页面镜像，下载策略单独拍板。

## 5. 人类要做的事

- 开通 EdgeOne Makers；创建项目（直接上传或 Git 集成，按选定路线）。
- 绑 `rb.cthun.com` 自定义域名；在腾讯云 DNS 加 CNAME；等 SSL。
- 如走 CLI：生成 API Token，加到 GitHub `EDGEONE_API_TOKEN`。
- 发版后目视验收大陆访问速度、页面与下载按钮。

## 6. Agent 要做的事

- 按选定路线改 `.github/workflows/website.yml`（加 EdgeOne 部署步骤或部署钩子）。
- 如需镜像下载：改 `website/src/data/release.ts` 的资产选择/URL 校验，补单测；必要时加 `SITE_URL` 按域名切换（默认不改，canonical 指主站）。
- 加 `website/edgeone.json`（仅 Git 集成路线）。
- 更新 `website/AGENTS.md` 的部署说明与 `memory/FUTURE.md` 状态。

## 7. 验收清单

- [ ] `https://rb.cthun.com/` 可访问，HTTPS 正常，样式/字体/图片完整。
- [ ] 页面版本号与 `https://raybend.cthun.com/` 一致；下载按钮指向正确资产。
- [ ] 发布新 release 后，两个域名都自动更新（不需要手动 redeploy）。
- [ ] 构建不依赖浏览器运行时访问 GitHub API（页面加载无 GitHub 请求）。
- [ ] 大陆网络访问正常；窄屏/中英文切换正常。
- [ ] 若做下载镜像：安装包可下载、校验和一致；未做则明确记录「下载仍走 GitHub」。

## 8. 待决事项

1. 路线：§4.1 双发（推荐）还是 §4.2 Git 集成 + 部署钩子？
2. 下载：仍走 GitHub / ghproxy / 仅 NSIS / COS+CDN？
3. `rb.cthun.com` 是否要独立 SEO 收录？默认 canonical 指主站，不单独收录。
4. EdgeOne 项目类型：直接上传（配合 CLI）还是 Git 集成（配合部署钩子）？

## 9. 参考

- EdgeOne Makers：https://pages.edgeone.ai/
- 配额：https://pages.edgeone.ai/zh/document/limits-and-quotas
- 构建设置：https://pages.edgeone.ai/zh/document/build-guide
- 自定义域名：https://pages.edgeone.ai/zh/document/custom-domain
- 部署钩子：https://pages.edgeone.ai/zh/document/create-deploys
- CLI：https://pages.edgeone.ai/zh/document/edgeone-cli
- GitHub Actions 部署：https://pages.edgeone.ai/document/use-github-actions
- 主站部署：`website/AGENTS.md` §5
- 取版与下载逻辑：`website/scripts/release-info.ts`、`website/src/data/release.ts`
