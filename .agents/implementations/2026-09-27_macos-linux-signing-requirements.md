# macOS / Linux 签名要求核实

完成时间：2026-09-27 13:26:47 CST

## 范围

崔总要为发行决策核对三平台签名要求（重点：macOS 是否"进商店就自动签"），并给出对现有签名机制的评价（垄断边缘、商业本质、时代遗产、应以"密钥持有者身份"替代硬件强制的设想）。核实事实落进 `docs/release.md` 新增 §8；价值判断保留在对话中，不写成项目论断。

## 涉及文件

- `docs/release.md`：新增 §8「macOS / Linux 的签名要求（远期参考）」，含三平台对照表、六条要点与七个来源链接。

## 核实到的关键事实

- **macOS 直下分发**：Developer ID 证书 + **强制公证**（2019-06-01 后构建的 Developer ID 软件必须公证；公证是自动化扫描，非 App Review）。**Apple Developer Program 会员资格是请求/下载/使用 Apple 颁发证书的前提**——Apple 是签名权的唯一来源，无第三方 CA 入口。
- **macOS Sequoia 15**：取消右键绕过 Gatekeeper，改为系统设置 → 隐私与安全 → "仍要打开"（Ars Technica 报道 Apple 原话）；Apple 支持文档把绕过描述为"Mac 中毒最常见方式"。
- **Mac App Store 不是"免签名通道"**：开发者需先有 $99 会员并自签（Mac App Distribution + Mac Installer Distribution），Apple 再重签；对比微软 Store 的 MSIX 是"认证后重签、可不买商业证书"。
- **App Store 强制 App Sandbox**：访问沙盒外目录需 entitlement 用户授权——对照片管理软件是产品层约束（比证书更重要）。
- **Linux 模型 = 密钥即身份**：deb/rpm 走仓库级 GPG（apt-secure 签 Release 文件；`dpkg-sig`/`debsigs` 为可选单包签名）；Flatpak 走 OSTree commit GPG 签名（`flatpak build-sign`，用户 `--gpg-import` 钉住公钥；OCI-backed remote 亦已支持）；AppImage/tar.gz 无需签名。无 CA、无身份验证、无硬件要求。
- **Sigstore**：Fulcio 依 OIDC 身份签发约 10 分钟短期证书 + Rekor append-only 透明日志（Merkle 树、可第三方验证）。**Windows/macOS 不认**，只认自家信任根——解决"可审计"但不解决"被放行"。
- **微软 Trusted Root Program 原文**：2024-02 起不再接受/认可 EV 码签证书，2024-08 起从现根移除 EV OID，"all Code Signing certificates will be treated equally"——信任判定是平台可单方面调整的政策。

## 成本排序（用于决策）

Linux $0 < macOS $99/年（唯一入口、全自动）< Windows $250–400/年（多 CA 竞争 + 硬件强制 + 信誉积累）。

## 验证方式

- 来源均为一手：Apple Developer 官方文档、Apple 支持文档、Ars Technica、Debian 官方手册、Flatpak 官方命令参考、Sigstore 官方文档、微软 Trusted Root Program Requirements。
- 文档改动仅 Markdown，markdownlint 通过。未执行打包/签名命令。

## 遗留问题

- macOS 沙盒对照片管理软件（库目录、外部编辑器、导入源）的具体 entitlement 可行性未评估——远期做 macOS 版时并入该平台的产品调研。
- 未核实 Flathub 提交的签名政策细节（本轮只确认 Flatpak 客户端侧的 GPG 模型）。
