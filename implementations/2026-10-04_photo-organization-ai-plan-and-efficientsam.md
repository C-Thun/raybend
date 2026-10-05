完成时间：2026-10-04 02:19:17 CST

# 相片整理 AI 标签实施规划与 EfficientSAM 调研

## 范围与交付

本轮按崔总要求只做实施方案与未来方向整合，不实施功能、不安装推理依赖、不下载权重、不生成/上传发布物。后续由崔总切换 sol 6.1 接手，没有创建或发送消息到其它会话。

- 重写 `specs/photo-organization-phase2.md`：有限物体/场景标签，CPU 推理，原始预览输入，多来源/文字屏蔽，有效标签统一投影，可恢复任务，模型包安装，XMP 接缝，UI/命令与验收。
- 新建 `specs/photo-organization-ai-w1.md`：仅细化首个模型/CPU/输入验证单元；模型、阈值、资源预算均标待实测。
- 更新 `specs/photo-organization.md` 与 `memory/PLAN.md` 的入口和当前状态，旧 L1–L4 草稿不再作为另一个方案。
- 更新 `memory/FUTURE.md` D4/E：合并原有局部蒙版与分割需求，登记 EfficientSAM 点选蒙版、CPU/ONNX 路径、6 点上限与编辑成果存储；不将蒙版加入本期整理。
- 按崔总最新说明修正 PLAN/FUTURE 的 XMP 当前状态。另一会话期间将规格更名为 `specs/xmp-sidecar.md`，本轮跟进新名，保留功能范围冻结边界。

## 关键决策

1. 标签和编辑蒙版分开模型，只共享可复用的推理、任务、安装基础；CUDA 不作产品前提。
2. TinyCLIP ViT-40M/32 首测，SigLIP 2 base 对照；量化/CPU/类别验证后只选一个默认包。MobileCLIP 的代码许可不能代替 research-only 权重许可。
3. 现有网格小图随 latest 编辑变化；识别需稳定原始 SOOC/RAW 内嵌预览，不触发全库完整 RAW 显影。
4. 复用现有 TagDialog、词典、来源表、PhotoRef、组织事件、自动桶与设置外壳；源 SQL 尚未具备统一屏蔽语义，需要实施时收敛，不能写成已完成。
5. `(manual ∪ valid_ai) − masks` 是所有消费者的唯一语义；自动桶仍累积，纠错不自动踢出历史成员。
6. XMP 正在开发，当前只约定有效关键词、自有来源/禁止往返及统一触发机制；不改 writer/读回代码，不重开跨家导入或另造队列，最终产品交付前必须完成联调。
7. EfficientSAM 官方源码确实依赖 torch；ONNX 路径可与产品运行环境分离。源码证明 CPU 路线可行，不代表已经在 Windows 验证了速度/质量。

## 实际查阅与验证

- 本地只读核对：organization/tag/query 及迁移、TagDialog、organization IPC、RAW worker、thumbnail 模块、GlobalSettingsDialog、XMP 现有接缝与最新规格。
- 官方源码核对：EfficientSAM 的 `efficient_sam.py`、`build_efficient_sam.py`、`onnx_models.py`、`export_to_onnx.py`、ONNX 示例、setup 与 LICENSE；来源链接集中在 FUTURE E2。
- `git diff --check -- specs/photo-organization.md specs/photo-organization-phase2.md memory/FUTURE.md memory/PLAN.md`：通过。
- Python 标准库文档检查：本轮五份文档末尾换行、无尾随空白、代码围栏配对、关键本地路径存在；FUTURE E6 及其后内容与本轮前快照相同：通过（DOC_CHECKS_OK）。
- 相对本轮前工作区快照检查 PLAN 差异：只更新 AI 交接与崔总澄清的 XMP 状态；其它会话的 XMP 更名、色彩等修改保留。
- 本轮仅文档，不运行 cargo/pnpm 产品质量门，不宣称新增单测/GUI/模型推理通过。未修改产品功能，命令和 XMP 接入要求已在方案写明，实际接入随各功能单元交付。

## 环境绕法与局限

沙箱内 exec 无法创建进程，node 文件通道也不能解析仓库路径；只读及文档编辑改用获准的沙箱外终端完成。apply_patch 的临时探测没有作为交付证据；其 `/tmp` 探测文件已通过终端核对并清除。未修改工具环境或项目配置。

文档有其它会话并行修改：第一次精确替换发现 XMP 标题/路径已变，断言在写入前停止；重新读取最新内容后再做局部合并，写前校验快照。临时旧内容存 `/tmp/raybend-ai-plan-before-20261004` 仅供本轮差异检查，不作项目进度源。

todo 工具在本会话不可用，未另造进度文件；spec 勾选项均为未执行的验收条件。

## 未验证与后续

- 标签模型的 Windows CPU 数值一致性、包体积、内存、吞吐与七类实际效果：AI-W1 执行。
- 新 AI UI 的 Pencil 定案：UI 开工前完成；上期批准不能替代新界面确认。
- XMP 最新接缝与来源/屏蔽往返：进入联动单元前重新读取、同次实现。
- EfficientSAM 的真实 ONNX 转换、Windows CPU 点选延迟、边缘质量与6点交互：未来蒙版单元执行，本期不实施。
