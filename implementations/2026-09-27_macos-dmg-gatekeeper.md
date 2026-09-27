# macOS 对 unsigned DMG 的真实态度：「判损坏」而非「弹警告」

完成时间：2026-09-27 14:21:57 CST

## 范围

崔总问：macOS 现在对直接安装 DMG 是什么态度——完全不让装（要求全走商店），还是能装但有类似 SmartScreen 的东西？核实后把关键细节补进 `docs/release.md` §8。

## 核实结论

**能装（商店不是唯一渠道），但 Gatekeeper 分三档处理，且对 unsigned 的处理比 SmartScreen 严酷一个量级：**

| 档位 | 首次打开体验 |
| --- | --- |
| Developer ID 签名 + 已公证 | 无任何提示，直接打开 |
| 有 Developer ID 签名、未公证 | 阻止；Sequoia 15 起右键绕过已移除，须「系统设置 → 隐私与安全性 → 仍要打开」多步操作；Apple 支持文档劝阻（称绕过是 Mac 中毒最常见方式） |
| **完全未签名** | **「XXX.app 已损坏，无法打开。您应该将它移到废纸篓。」**——虚假报错（app 没坏）；唯一解除方法是终端执行 `xattr -r -d com.apple.quarantine`，普通用户不可能完成 |

要点：

- **Windows 的 unsigned 有轻量逃生门（更多信息 → 仍要运行，两下）；macOS 的 unsigned 对普通用户等于不可安装**（「已损坏」+ 终端命令）。逃生门（系统设置多步流程）只在「有签名未公证」这一档存在。
- 真实案例：MarkText（ad-hoc 签名未公证 → 用户见 "MarkText is damaged"）、OVO、FreeAgent、LocalAI 等开源项目均有记录；Apple 开发者论坛 DTS 工程师明确表示不指望用户自行移除 quarantine。
- 机制（Mac Internals 技术文章）：quarantine 扩展属性（≈ MotW）→ 签名检查 → 公证检查 → 需要时弹窗。公证票据 staple 到 DMG 后，挂载时 Gatekeeper 会将其应用到内部 app（离线可用）。
- macOS 26 Tahoe 仍在持续修补 Gatekeeper 绕过类 CVE；Tahoe 是最后支持 Intel 的版本，社区（eclecticlight）预期代码签名政策进一步收紧——方向只会更严。

## 涉及文件

- `docs/release.md` §8：① 特性表「未签名直下体验」行 macOS 格改为三档描述（含「已损坏」）；② 要点首条重写：unsigned =「判损坏」而非「弹警告」，逃生门只在「有签名未公证」档存在，解除需终端命令。

## 验证方式

- 来源：Apple 开发者论坛（692774、799110）、Homebrew issue #17979、MarkText PR #4917、LocalAI PR #10606、OVO issue、Apple 支持文档、macOS Tahoe 26 安全内容（多个 Gatekeeper bypass CVE）、eclecticlight（2026-01-17）。
- 文档改动仅 Markdown，markdownlint 通过。未执行打包/签名命令。

## 遗留问题

- 无新增。结论与既有决策一致：远期 macOS 走「Developer ID + 公证」直下分发，$99/年为硬预算。
