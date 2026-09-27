# SmartScreen 信誉跨版本规则核实

完成时间：2026-09-27 12:49:01 CST

## 范围

崔总问「完全没有证书是不是永远绕不过 SmartScreen，即使信用很高」。核实微软官方规则，结论补进 `docs/release.md` 证书路线对照小节（含来源）。

## 涉及文件

- `docs/release.md`：追加「SmartScreen 信誉的跨版本事实」五条要点；来源行补 SmartScreen 官方页与 Eric Lawrence 实操文章。

## 核实到的关键事实（一手来源）

- **官方原文**（learn.microsoft.com smartscreen-reputation）：「When a file is not signed, SmartScreen reputation must build for each new version of your files, starting with zero reputation. **Reputation cannot transfer from previous versions unless both were signed using the same publisher identity.**」
- SmartScreen 评估两个信号：发布者信誉（需签名证书）与文件哈希信誉（具体一个文件）。未签名文件只剩后者，而每次发版哈希必变。
- 未签名文件**不是永远无法过**：同一个哈希被大量下载且无恶意迹象可进入 Known Good —— 但那个信誉只属于那个确切文件，不随版本走。
- 签名不等于立即放行：每张证书自己攒信誉；EV 自约 2019 年起不再特殊对待（微软官方明确「EV no longer bypass SmartScreen」）。
- 换证书按新发布者身份处理；缓解手段（Eric Lawrence 2024-11，2026-09 仍更新）：新证书保持相同 Subject（CN/O/L/S/C）、提前用新证书签名并通过 Defender 提交门户的 "Software Developer" 流程登记。
- 唯一确定性绕过：Microsoft Store（MSIX），商店重签，永不触发下载警告。Windows 11 Smart App Control 更严：未签名文件直接阻止。
- 无消费者端人工白名单/查询 API（Eric Q3）。

## 策略含义（本轮最重要的结论）

**「先用 unsigned 发布攒信誉、以后再买证书」是无效策略**——未签名阶段积累的文件信誉在签名后一分也带不走，签名版本从零开始。未签名发布的价值是「现在就能发布」，不是「攒信誉的前置阶段」。

## 验证方式

- 只改 Markdown，编辑工具内置 markdownlint 通过。无代码改动。
- 未执行打包/签名命令。

## 遗留问题

- docs/release.md 只记录规则与策略含义；发行文案（官网/RELEASE-NOTES）若未来提到「信誉积累」需按本节口径，不得暗示未签名阶段能预存信誉。
