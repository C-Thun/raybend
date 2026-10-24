# 国内官网镜像移入「发版后」并落方案 spec

完成时间：2026-10-24 13:48:55 CST

## 改动范围

按崔总指示，把「国内官网镜像（`rb.cthun.com` / 腾讯 EdgeOne）」从 `memory/FUTURE.md` 的
**Future Release #6（发版前必须完成）** 移到 **「发版后（after release）」** 板块，并把方案细节
从 FUTURE 抽到独立规格，降低 FUTURE 的正文厚度。

## 涉及文件

- `memory/FUTURE.md`
  - 删除 Future Release 表 #6 行；保留 #7–#10 原编号，并在表下注明「编号保持历史稳定，原 #6 已移入发版后」。
  - 「发版后」板块新增国内镜像条目，指向 `specs/website-cn-mirror.md`。
  - G24 补充从 5 条长 bullet 压缩为一段结论 + 指针（Gitee/Deno 排除、EdgeOne 选型、备案已满足、方案见 spec）。
  - 顺手把 Future Release #5 的取版描述从「前端 JS（XHR/fetch）实时取」修正为**已落地的构建期注入**
    （`website/scripts/release-info.ts`，`release: published` 触发重建；XHR 方案未采用），避免与现状不一致；
    G24 主条目末尾同步补注这一点。
  - 变更记录新增 2026-10-24 行。
- `specs/website-cn-mirror.md`（新增）
  - 背景、现状、选型与免费版配额、两条落地路线（推荐 GitHub Actions 构建一次双发；备选 EdgeOne Git 集成 + 部署钩子）、
    下载策略、人类/Agent 分工、验收清单、待决事项、参考链接。
- `implementations/2026-10-24_cn-mirror-after-release.md`（本文件）

## 关键决策与理由

1. **移到 after release**：该事项不再阻塞 release；首发 v0.1.3 已发出，继续挂在「发版前必须完成」会失真。
2. **不重排 Future Release 编号**：#10 等旧引用仍有效，删除 #6 后保留空号并用表下注说明，避免连带改历史记录。
3. **FUTURE 只留指针**：详细查证（配额、构建环境、刷新时机、下载限制）放进 spec；FUTURE 只保留一句话现状与待拍板项。
4. **推荐路线写进 spec**：优先「GitHub Actions 构建一次 → Pages + EdgeOne 双发」，避免 EdgeOne 境内二次构建时
   受 pnpm 12.3.4、`api.github.com` 可达性与 release 触发缺失影响；原 Git 集成方案保留为备选。
5. **下载策略单独拍板**：EdgeOne Makers 免费版单文件 25 MB，当前 latest MSI 约 120.5 MB，无法直接镜像；
   NSIS（约 15.5 MB）可放但当前 latest 没有；`pickInstaller` 只接受 GitHub 资产 URL，改镜像/代理需动代码与测试。

## 验证与事实核对

- `dig rb.cthun.com` → NXDOMAIN（尚未解析）；`dig raybend.cthun.com` → CNAME `c-thun.github.io`（主站正常）。
- GitHub API `repos/C-Thun/raybend/releases/latest` → v0.1.3，资产仅 MSI 120,549,376 B（无 NSIS）。
- EdgeOne Makers 免费版配额、构建环境、部署钩子/CLI 均按官方文档核对，链接写进 spec §9。
- 本次只改文档，无代码/测试改动；`write`/`edit` 的 Markdown 检查通过，spec 章节与 FUTURE 指针已 grep 核对。
- 未做：EdgeOne 建站、域名 CNAME、API Token、CI 部署步骤（均待路线与下载策略拍板后开工）。
