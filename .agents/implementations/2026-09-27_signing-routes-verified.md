# 签名路线核实并补进发布文档

完成时间：2026-09-27 12:33:37 CST

## 范围

崔总问「申请到签名之前是否先用 `--unsigned`」与「签名是否只有这几个选项」。核实仓库脚本现状 + 各签名服务当前政策，结论落进 `docs/release.md`（发布操作的事实源），不新增第二份签名文档。

## 涉及文件

- `docs/release.md`：§1 末尾追加「证书路线对照（2026-09-27 核实）」小节（表格 + 来源链接）。

## 脚本现状（已读代码核实，不是推测）

- `scripts/lib/release-windows.mjs:6`：`!unsigned && !/^[a-fA-F0-9]{40}$/.test(thumb)` → 抛错「Windows 签名需要 RAYBEND_SIGN_CERT_SHA1……无签名显式 --unsigned」。**`--unsigned` 是门禁，不是可选习惯**：未配置指纹且未写该参数时，第一条（`release`）在改版本号之前就失败。
- 同文件 `:19`：`signCommand` 硬编码 `signtool.exe sign /fd SHA256 /td SHA256 /tr <ts> /sha1 <thumbprint> %1` —— 只支持**证书在本机 Windows 证书存储**的形态（Token 中间件 / 导入 .pfx）。云签名服务（SignPath / Azure Artifact Signing / SSL.com eSigner）都要替换这个命令，即「另一层适配」。
- `scripts/release-publish.mjs`：第二条只接受 `<目录>` 与 `--execute|--dry-run`，**不涉及签名参数**。
- `scripts/lib/release-upload.mjs:14`：`authenticode` 合法值就是 `unsigned | verified` —— **unsigned 不阻断发布**，只是如实写进 `release-index.json` 与发布说明。

## 本轮核实到的时效信息

- **Azure Artifact Signing（原 Trusted Signing）对本项目不可用**：官方条款为组织限 US/CA/EU/UK/AU/NZ/JP/KR/SG/CH/NO/IL，个人开发者仅 US/CA，中国大陆不在名单（来源：learn.microsoft.com 的 artifact-signing quickstart）。此前文档只写了「$9.99/月」，未记地区限制。
- **SSL.com IV（Individual Validated）**：个人主体可申请、无需公司注册，约 $379/年；可另配 eSigner 云签名（Tier 1 年付约 $15/月）免 Token。这条补上了「个人开发者」这一档，此前文档只有 OV/EV 的笼统说法。
- **Certum Open Source Code Signing 不是免费的**：约 €49 起（或 €25 需自备其加密卡与读卡器），且官网当前显示 out of stock —— 是开源折扣价，不是免费额度。
- **新兴免费开源签名服务**（OSSign、Necessary Code Signing）确实在运营，但签发主体与可持续性未核实，只登记为观察项。
- 重申既有结论：EV 不绕 SmartScreen，不必加钱；自签不适合公开直下。

## 验证方式

- 只改 Markdown：编辑工具的内置 markdownlint 通过（「Markdown clean」）。未跑代码测试（无代码改动）。
- 脚本现状结论均来自直接阅读上述文件，未执行任何打包/签名命令（AGENTS.md：省略 `--dry-run` 的打包命令不由 Agent 执行）。

## 遗留问题

- 签名主体（个人姓名 vs 组织）未定，直接决定可选路线（IV vs OV）与显示出的发布者名称。
- 云签名适配（若选 SignPath / SSL.com eSigner）需要在 `release-windows.mjs` 的 `signCommand` 上新增分支，并重跑真机验收；本轮未做。
- 证书到手前，公开版继续走 `--unsigned`，官网文案保持「未签名/未知发布者」的如实说明。
