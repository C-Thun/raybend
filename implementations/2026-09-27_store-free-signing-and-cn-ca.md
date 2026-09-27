# 微软商店签名与费用再确认、国内证书通道核实

完成时间：2026-09-27 14:08:40 CST

## 范围

崔总要走发行决策，提出按「安装体验 / 签名」两层框架核对，并要求：① 再确认 MSIX 进微软商店是否免费给签；② 核实国内有没有 OV 供应商。本轮把这两块查实并补进 `docs/release.md`（证书路线对照加「国内代理通道」行；§6 补商店签名与费用）。

## 核实到的关键事实（均为一手来源）

### 微软 Store：比预期更优，两条都免费

- 官方原文（app-package-requirements）：「Your MSIX and AppX packages don't have to be signed with a certificate rooted in a trusted certificate authority when submitting to the Microsoft Store. The Microsoft Store will automatically re-sign your MSIX/AppX packages with a Microsoft certificate during the publishing process after your app passes certification.」
- 开发者账号费用已归零：个人 $19 于 2025-09 豁免（Free developer registration for individual developers，近 200 市场、无需信用卡、政府身份证+自拍验证）；公司 $99 于 2026-05 豁免。
- 两个边界：只有 MSIX/AppX 免签；用 MSI/EXE 走商店仍需自己 Authenticode 签名。Tauri 原生只出 NSIS(.exe)/MSI，所以需要先做 MSIX 包装工程。

### macOS App Store（回应「是否只是进商店不给签」）

- 进商店必须**先用开发者自己的证书签**（Apple Distribution / Mac Installer Distribution，来自 $99 会员）。
- Apple 官方与 electron 文档：「apps signed with the Apple Distribution certificate cannot run directly, they must be re-signed by Apple to be able to run, which will only be possible after being downloaded from the Mac App Store」——商店是最终重签，不是免签通道。
- 上架 App Store 不需要公证（提交流程含等效安全检查）；直下分发则强制公证。
- **不存在「外面找供应商签 macOS」这条路**：Developer ID 证书只能 Apple 颁发。

### 国内证书通道

- 国内**无自主公开信任码签 CA**：微软 2019-03 正式废止 WoSign/StartCom 全部 5+2 张根证书（安全内参报道）。
- 现有玩家均为**代理商**：沃通 WoSign/WoTrus（代理 Certum/DigiCert 等）、天威诚信 iTrusChina（DigiCert/VeriSign 中国区官方合作）、零信 ZoTrus（代理 Sectigo）、火山引擎证书中心。
- **个人主体只能买 IV**：OV/EV 需组织实体（沃通 EV 产品页原文：「EV代码签名证书仅限于单位用户申请」）。零信明确提供面向「未注册公司的个人软件开发者」的 IV。
- 零信通道特点：国产 UKey 深圳顺丰快递（24 小时送达）+ 中文电话鉴证，省掉跨境身份验证与 Token 国际邮寄。
- 沃通含税报价（1 年）：标准 OV 3588 元 / OV Pro 4888 / EV 4288 / EV Pro 6888；多年更便宜。对比：SSL.com IV $379/年、DigiCert 官网 $996/年——国内代理不更便宜，但流程更短。
- 微软 Trusted Root Program 条款附注：码签**不支持 ECC 或密钥 > 4096**；买证书时要选 RSA。

## 验证方式

- 来源：Microsoft Learn（app-package-requirements、code-signing-options、open-developer-account、whats-new-individual-developer、whats-new-company-developer）、Windows 开发者博客（2025-09-10、2026-05-07）、Apple 官方（notarizing、certificates）、electron 文档、沃通价格页与产品页、零信产品页、安全内参（WoSign 废止）。
- 文档改动仅 Markdown，markdownlint 通过。未执行打包/签名命令。

## 遗留问题

- 零信/ZoTrus 的 IV 具体报价与验证材料清单未取（页面未列价，需询价）；若崔总决定走国内通道，下一步是直接询价与确认微软信任链。
- MSIX 包装工具链选型（MakeAppx vs 第三方）属 FUTURE 的 MSIX 条目，本轮不做。
