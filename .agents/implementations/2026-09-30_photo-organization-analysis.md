完成时间：2026-09-30 02:51:43 CST（Asia/Shanghai）

# 相片整理第一轮算法评审与实验

## 改动范围

仅新增待评审分析与可复现实验。没有修改业务代码、schema、Pencil、原提案或计划/决策记忆；没有提交、推送或构建发布物。会话开始时已有编辑预设等工作区变更，本轮均保留。

文件：

- `specs/photo-organization-analysis.md`：文字语义、来源与遮罩、目录关联、查询、桶身份、离线行为、自动桶一致性、UI 复用与裁剪建议。
- `specs/experiments/photo-organization/bench.mjs`：仅使用 Node 自带 SQLite 的内存合成实验，不连接真实 app/catalog。
- `specs/experiments/photo-organization/100k.json`、`1m.json`：10 万与 100 万照片的实验结果。

所有新增产品选择保持“待崔总评估”，没有把建议登记成已采纳。

## 关键结论与复用

- 标签合并建议 `(manual ∪ directory ∪ ai) − masks`；文字 key 为现有 Rust 规范化结果，目录来源不逐照片物化。
- 查询实测表明稀有词适合反向索引、常见词首屏适合按排序索引有限探测；建议 4096 条上限的探测再回退候选查询。
- catalog 现有关联只存全局数字 tag ID；名称迁移必须有可靠映射，不能用补 ID 假装恢复文字。
- 现有重扫保持匹配资产 ID，FileId 属物理文件而非照片；建议跨库桶引用持久照片 UID，并明确裸文件重建不能恢复丢失关联。
- 目录 `_RAW` 折算复用既有照片归属规则；分隔条复用 `SplitStack / SplitHandle / stack-resize`；自动桶可靠性扩展 `rebuild::Pending` 这类现有提交补偿通路。没有新增产品实现副本。
- 建议暂缓全局改名/合并、同义词、全量实时标签计数、复杂条件树、跨库全球排序、AI 推理。

命令体系评估已在分析稿 §9 写明：未来用户动作登记统一命令表，新增动作默认热键明确留空（低频、有现成面板入口、避免占键），标签添加复用现有入口。本轮只是分析与手动性能实验，不新增用户命令。

## 已验证（冒烟/分析实验）

实验运行时脚本位于 `/tmp/raybend-photo-organization-bench.mjs`，运行后保留同内容到 specs/experiments；Node v26.5.0、SQLite 3.53.3、pnpm 12.6.0。

```bash
# 当时在 /tmp 执行，结果随后存入仓内实验目录
pnpm exec node /tmp/raybend-photo-organization-bench.mjs 100000
pnpm exec node /tmp/raybend-photo-organization-bench.mjs 1000000

# 保留文件的语法检查（通过）
pnpm exec node --check /home/andares/repos/c-thun/raybend/specs/experiments/photo-organization/bench.mjs

# 复现实验：同样从 /tmp 执行即可，无须安装新依赖
pnpm exec node /home/andares/repos/c-thun/raybend/specs/experiments/photo-organization/bench.mjs 100000
```

- 断言通过：两种查询完整结果相等，双路径首屏相等，来源去重、来源移除补位、遮罩持续生效、解除遮罩恢复。
- 10 万数据在百万实验完成后单独复跑，保存的是最后一次结果，减少并发负载干扰；所有数字只是本机样本中位数。
- 保存查询计划；直接来源、目录与文件关联、遮罩均有预期索引查找。
- `git diff --check` 通过（仅检查已有 tracked diff；本轮新文件另外进行语法/文本检查）。
- 无产品代码改动，没有运行 cargo/frontend 产品质量门；重负载实验不接默认秒级单元测试。

## 绕过的问题与风险

- 仓内 `pnpm --version` 为核实项目锁定的 12.3.4 尝试访问包注册表，受限网络导致签名信息核实失败，最终报告本机 12.6.0。改从 `/tmp` 运行无依赖实验；未修改 packageManager/锁文件、未安装包。此绕法不验证项目锁定工具链。
- Firecrawl 技能状态检查显示已认证但网络 fetch 失败；改用内置 web 只核对 SQLite 与 Microsoft 一手资料，链接写在分析稿。没有改网络配置或索取新授权。
- 当前工具列表没有 pi todo；本轮未建立另一份任务进度表。

## 未经崔总验证/遗留边界

内存 SQLite 合成实验不是 Windows 真库、磁盘 WAL、Rust 查询集成、IPC 或 GUI 性能验证；没有把进程/脚本成功当成产品正确。

尚待评估的取舍完整列于分析稿 §11。上下区域高度、窄窗降级、来源详情、桶视图需后续 Pencil 稿定案；自动桶瞬态命中降级、目录路径绑定、标签改名裁剪都没有自行拍板。
