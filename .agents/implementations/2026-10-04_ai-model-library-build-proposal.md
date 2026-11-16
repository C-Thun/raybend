完成时间：2026-10-04 16:02:46（Asia/Shanghai，UTC+08:00）

# 独立 ONNX 仓库与可选 AI 构建：方案报告

## 范围与状态

本轮按崔总要求完善方案，待评估后提供独立仓库路径再实施。只新增需求原文与 spec；没有继续上一轮暂停的构建脚本修改，没有导出、创建外部仓库、提交权重、推送、tag、打包或发布。

涉及文件：

- `todos/2026-10-04-ai-model-library-and-build.md`：保存完整需求原文及落点，不作为进度表。
- `specs/ai-model-library-build.md`：一个工作单元的方案与验收清单，显式标为待评估。
- 本实施记录。

上一轮下载接线尚未验证/交付，当前源码仍有默认启用 CPU 推理、资源预检强制要求模型和 Tauri 无条件模型资源装配。新方案不能作为现有功能宣称；后续统一改成这里的本地源登记/消费流程，不把两种来源判断并存。

## 关键决定与理由

1. 推荐独立模型仓库，完整模型包以 manifest 摘要不可变归档；模型版本独立，软件兼容与可信摘要仍明确固定。151.53 MiB 编码器超过 GitHub 普通 Git 的 100 MiB 限制，匹配“导出后提交”的方案采用 Git LFS，不把大文件普通提交后留下无法推送的历史。
2. 模型制作与应用构建分离；模型库根标记+UUID、显式初始化、工具专用默认路径，拒绝接管任意目录。已存在且受信的包直接复用，同名损坏不覆盖；校验与原子发布完成后才提交和登记源。
3. Git 自动提交只对库根，只添加本次路径，无关 dirty/index/HEAD变化拒绝；非 Git 库同样可用。提交失败保留完整包、不改旧源指针，不 reset 或混入外部修改。
4. Git ignored 本地选择文件为 `ai-model-source.local.json`；增加只登记现成模型的 `ai:use` 以支持换机/搬家而不重复导出。第一波只做本地源，无虚构下载地址和隐式 pull/导出。
5. 构建默认 auto：配置缺失或 AI 输入不可用，提示原因、构建无 AI；required 可供正式构建确保能力，off 明确跳过。普通编译/签名/核对错误不能吞掉并降级。
6. 真正无 AI 要同时控制 Rust feature、平台运行库/模型资源、前端入口、命令和 worker/IPC拒绝；由同一个构建计划装配，产物自带 capability/资源身份。核对旧 exe 时不能根据当下配置反推能力，AI→无AI→AI切换列为验收重点。

## 检查依据

- 阅读现有模型导出/包装/标签配方、manifest/许可证、可信包 registry、core/desktop features 和 AI 薄壳 cfg、前端 AI 入口/命令，以及 debug/release/Tauri resources/镜像/产物核对实现；已有基础足以复用，未另造后端或设置组件。
- `pnpm tauri build --help`、`pnpm tauri bundle --help`：核对 CLI feature/config 装配接口；只有读取帮助，没有构建或打包。
- [GitHub 大文件限制](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github)、[Git LFS](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-git-large-file-storage)、[模型附件可使用 Releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases)：读取官方说明，不使用第三方推测。
- 文档限定范围 `git diff --check` 与 Node 逐行检查：新增原文/spec 无行尾空白，末尾换行正确。纯方案文档没有运行前端/Rust测试，也没有宣称实现验证通过。

## 命令、XMP 与待确认事项

构建 CLI 不进入产品命令面板；已有 AI 四命令应随编译 capability 控制可用性，沿用默认热键留空及明确范围的理由。未来 UI 的无 AI 状态复用现有页面，并按 Pencil 纪律先补状态再实施。

模型/构建开关不修改 XMP schema；无 AI 构建仍保留旧 AI 来源标签、禁止/纠错和任务记录的读取，必须继续经过已有 XMP 往返测试，不以“不启用推理”删掉数据模块。

崔总评估方案后创建并 clone 独立模型仓库、提供路径；届时实现与验证整个单元。公共发布不是本地可用的前提，不要求当前就有 HTTPS 制品地址。GUI/真实图库/安装器切换与干净机环境最终由崔总确认。
