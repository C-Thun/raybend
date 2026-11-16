# 开源生态签名现状抽样与 Tauri 官方口径

完成时间：2026-09-27 13:13:01 CST

## 范围

崔总问两件事：① GitHub 带下载的客户端软件是否基本 unsigned；② Tauri 官方对签名的建议。做了三路核实：一手抽样（PE 证书表 + openssl 解链）、firecrawl 搜索生态物证、Tauri 官方文档与官方讨论区。结论落进 `docs/release.md`。

## 一手抽样方法与结果

方法：GitHub API 取各仓 latest release 的 exe 资产 → HTTP Range 仅取文件头 8KB → 解析 PE Security Directory → 若有证书表再 Range 取那一段 → openssl 解 PKCS#7 提取 subject/issuer 判定是否公开信任（区分自签）。**不下载完整文件**。

| 项目 | 结果 |
| --- | --- |
| Notepad++ x64 (v8.9.8.1) | ✔ GlobalSign OV（subject O=NOTEPAD++，issuer GlobalSign GCC R45 CodeSigning CA） |
| OBS Studio / HandBrake / VSCodium / Jan / Yaak | ✔ 公开信任签名 |
| **7-Zip x64 (26.03)** | **无证书表 = UNSIGNED**（实测确认） |
| **qBittorrent (5.2.3)** | **无证书表** |
| **ShareX (21.0.0)** | **无证书表** |
| **Spotube（Tauri）** | **无证书表** |
| KeePassXC / Spacedrive | MSI，PE 法不适用，未判定 |

12 个知名项目：6 签 / 4 未签 / 2 未判定。结论：**头部项目约六成签名；"大多数 unsigned"在长尾成立、在头部不成立**。

## 生态物证（firecrawl 搜索）

- 文件名带 `-unsigned` 的实例：SMPlayer（`smplayer-26.8.29-x64-qt5.6-unsigned.exe`）、Block 公司的 Buzz（`Buzz_0.5.25_x64-setup_alpha-unsigned.exe`）、torum/Image-viewer（README 明说提供 unsigned 版）、komi-store（release note 写 "ships unsigned .exe/.msi"）。
- signed/unsigned 双发的实践：LanternOps/breeze issue #3453（把 unsigned 误标为 signed 引发 bug 报告，反证双发是常规操作）。
- Notepad++ 证书史：DigiCert 捐赠 9 年 → 2025-05 到期续签被拒（无注册商业实体）→ v8.8.3–8.8.6 自签（视同未签名）→ 现获 GlobalSign OV。
- ShareX issue："ShareX has been unsigned for decades. It doesn't have enough funding to justify signing it."
- GitHub 官方社区请愿帖（orgs/community/discussions/4293）要求 Microsoft/GitHub 为开源项目提供签名支持。
- Reddit r/tauri："Windows code signing is broken for indie developers outside US/Canada"。

## Tauri 官方口径

- v2 文档（v2.tauri.app/distribute/sign/windows/）开篇：签名"不是 Windows 上运行的必要条件，只要最终用户愿意忽略 SmartScreen 警告"；签名是 Store 上架所需 + 避免 SmartScreen 提示。官方路径：OV+signtool 证书存储（certificateThumbprint）、Azure Key Vault（relic）、Azure Artifact Signing（artifact-signing-cli）、自定义 signCommand。**未提 SignPath**。
- 官方讨论 #8046（维护者 Fabian-Lars + 2026 更新评论）：EV/OV 证书"stupidly expensive"；EV 自 2024 年起与 OV 同等待遇；实操 = "sign every release with one certificate and keep it, expect the prompt on early downloads of each release, and do not buy EV"；唯一彻底消除提示 = Microsoft Store（重签）。自签证书"same behavior as no signature"。
- Crawlix 2026 实测数据（参考量级，非权威统计）：unsigned 安装完成率约 62%（配清晰预安装说明约 78%），签名应用基线 88–94%。⚠ 该文有一处技术错误（称 2023-06 新规为"必须 EV 级"——实际是私钥硬件存储，OV 同样适用），已按 CSBR 原文纠正采信。

## 对本仓的映射

- `release-windows.mjs` 的 signCommand（signtool + `/sha1` 指纹）与 Tauri 官方第一路径完全一致；云签名走官方 signCommand 自定义机制即可扩展。
- Tauri 维护者口径与本项目既有决策一致：不买 EV、一张证书长期持有、unsigned 发行可接受。
- 首批 12 样本脚本运行正常（几秒）；第二批脚本因 Range 写成 `0-证书表末尾` 导致每次下载整个安装器（数百 MB）而卡 6 分钟，已废弃并修正为精确区间——教训：Range 请求必须只取目标区间。

## 验证方式

- 抽样脚本：node + fetch Range + openssl pkcs7（WSL 本地，无全文件下载）。
- firecrawl search/scrape：生态物证与 Tauri 讨论原文。
- 文档改动仅 Markdown，markdownlint 通过。

## 遗留问题

- 未对 MSI 资产判定签名（OLE 复合文档需另一套解析）；后续真机验收时可用 signtool verify 补。
- 抽样仅 12 个头部项目，未覆盖长尾；"长尾大多 unsigned"基于间接证据（文件名物证 + 社区讨论 + Notepad++ 案例）。
