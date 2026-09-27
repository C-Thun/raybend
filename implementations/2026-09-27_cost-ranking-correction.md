# 更正：「按平台排序签名成本」是错误结论

完成时间：2026-09-27 14:19:00 CST

## 范围

崔总指出我在对话中说过「Linux $0 < macOS $99/年 < Windows $250–400/年，Windows 才是三平台里最贵最麻烦的」，并认为这是误导。**该判断成立，是我的错误**，且同一句话已写进 `docs/release.md` §8 开头，必须更正。

## 错误性质（不是信息缺失，是汇总模型用错）

- 该排序只在「**直下分发 + 自行签名**」这一条路径上成立；它把「Windows 的成本」等同于「Windows 直下自签的成本」。
- 而 Windows 还有两条 $0 路径：**unsigned 直下**（代价是流失）与 **MSIX → 微软商店**（微软重签，**开发者账号也已免费**）。第二条恰恰是解决「流畅安装」的路径，却被漏掉。
- 更该批评的是：这条信息并非不知道——早在核实 SmartScreen 时我就引用过微软原文「Store-distributed apps are signed by a Microsoft certificate and are never subject to SmartScreen download warnings」，也在更早的对话里说过 MSIX 商店重签。**是汇总时用了不完整的模型**。
- 正确的成本单位是「**平台 × 分发路径**」，不是「平台」。

## 涉及文件

- `docs/release.md` §8：① 删除「按开发成本排序：Linux < macOS < Windows」整句，替换为「平台 × 路径」成本矩阵（五行）；② 特性表「成本与硬件」行 Windows 格补「MSIX 商店路径 $0」；③「商店路线」行 Windows 格改为「微软重签，$0，开发者账号也已免费」；④ 要点首条删去「好在它比 Windows 证书便宜」，改为「这一点比 Windows 更硬：Windows 至少有两条 $0 路径，macOS 一条都没有」。

## 更正后的正确结论

| 平台 × 路径 | 现金成本 | 代价 |
| --- | --- | --- |
| Linux 直下 | $0 | — |
| Windows 直下 unsigned | $0 | 每版流失约 22–38% |
| Windows 直下 + 自签 | $250–400/年 | 硬件 + 攒信誉 |
| **Windows MSIX → 商店** | **$0** | MSIX 包装工程 + 双通道构建 + 商店接管更新 |
| macOS 任何路径 | **$99/年** | 唯一入口；直下需公证，商店需沙盒 |

即：**Windows 有一条完全免费的顺畅安装路径（MSIX 商店），macOS 的钱一分不能省。**

## 验证方式

- 事实依据均来自前两轮已核实的一手来源（微软 Learn：app-package-requirements、code-signing-options、open-developer-account、whats-new-individual-developer、whats-new-company-developer）。
- 文档改动仅 Markdown，markdownlint 通过。

## 遗留问题

- 结论变更后需重新审视的决策点：若把「官网直下顺畅安装」列为目标，**MSIX 商店路线的工程成本（而非证书成本）成为唯一瓶颈**，这应在 MSIX 条目启动时作为主要估算对象；崔总若要求，再登记进 FUTURE.md 的 MSIX 条目。
