完成时间：2026-10-04T18:10:58+08:00

# 独立模型库与可选 AI 构建交付

## 改动范围

模型仓库 `/home/andares/repos/c-thun/model-registry` 已建立 AGENTS.md、readme.md、归属标记、LFS/忽略规则、固定 CPU 配方、导出/验证/发布工具及单元测试。三级模型定位 `onnx/wkcn-tinyclip-vit-40m-32/hf95ec8197-op17-fp32-r1`，项目配置单独 `profiles/raybend-photo-tags/v1`；同一编码器可被不同 profile 复用。LFS 实体151.53MiB，Git存指针。首次制品提交 a11b522，工具后续修正均局部提交，未推送。

RayBend 涉及 `.gitignore`、`package.json`、`scripts/ai/registry.mjs`、`scripts/ai/prepare-assets.mjs`、`scripts/ai/runtime-assets.mjs`、`scripts/lib/ai-{assets,build,prepare}.mjs`、`scripts/{tauri,debug-win,release,check-win-artifact}.mjs`、release mirror/windows helpers及其测试；Rust `ai/mod.rs`/worker常量、desktop Cargo features/build.rs/main/photo_ai；前端 capability/API/DTO、App、BrowseToolbar、AiModelSettings、命令目录及i18n。同步规格、设计配套说明、许可与模块记忆。

## 核心决定

- 制作与打包分离：`ai:export [模型库路径]` 才下载固定原权重/导出ONNX。默认用户数据目录可非Git；明确路径需已初始化的自己的库。Git库需要LFS、attached branch、身份、干净边界和稳定HEAD/index；只提交列出的制品文件，不把版本目录内其它文件一起提交。失败保留已验证产物，旧项目指针不更新；同名版本不覆盖、不制造空提交。
- 首次制作预检 Node22.12、Git/LFS/身份/状态、Python3.12/venv/固定CPU依赖和至少4GiB空间。Python在独立环境内，不污染全局。暂存、锁、profile输入与路径都有边界检查；拒绝链接逃逸。制作日志走stderr，stdout只有供适配器读取的JSON。权重与模型不进软件Git。
- 本地源 `ai-model-source.local.json` 已登记并被Git忽略；`ai:use`仅消费已有artifact/profile，不依赖制作工具。普通构建不导出、不修改模型Git、不隐式pull。软件对既有准入manifest与文件摘要再次校验，不让自报身份替换可信列表。
- 统一 auto/required/off：输入不可用默认提示并基础版；required在版本/前端写入前失败；off跳过源/网络。Windows CPU ORT采用固定官方ZIP、大小与摘要验证、缓存和仅两DLL提取。Vite、Rust feature、Tauri资源与产物记录共享一次冻结计划；独立bundle必须匹配已有exe的能力和输入。
- 原生默认无推理依赖；关闭AI保留全部数据结构/旧标签/禁止/纠错/XMP，不领取推理任务；识别/重识别/任务/模型命令按capability隐藏，保留基础模型设置说明。安装/提交/重试IPC与worker入口均由原生feature拒绝。编译标记与产物记录是核对依据，不能按此机器当前源猜旧exe。
- 当前完整产物 `C:\rb-target\raybend\debug\raybend-desktop.exe` 为有AI，标准debug流程通过。关闭AI副本 `C:\rb-target\raybend-registry-test-off\raybend-desktop.exe` 保留供验收。

## 验证（Agent 冒烟）

1. 模型库 `pnpm test` 11/11；真实Git LFS含初始未提交分支、HEAD/index竞争、失败hook恢复、版本内外来文件拒绝、无空提交、路径/链接/锁/同名不可变、两个profile复用一个编码器。测试无跳过。
2. RayBend `pnpm test:release` 78/78，含本地源原子登记/旧指针保护、LFS未检出、路径和元数据界限、冻结后库变化、auto/required/off、仅有效模型准备运行库、ZIP损坏/重定向/离线缓存/并发、资源on-off-on、未知文件清除、dry-run无副作用、真实构建失败不被降级、Tauri feature/bundle一致性。
3. 固定 CPU 真实重导出158885258 bytes，SHA a8576f0f9916d6e92237996d41c256e1e3150db5cf6db48758bb63c87537bb24；PyTorch/ORT最大绝对误差8.30e-6，小于1e-4，1×512有限向量。指定库导入/自动提交/重复复用及默认XDG隔离目录实际导出成功，已恢复本机引用相邻Git模型库。使用上游固定revision缓存，不重复下载已有权重。研究适配入口与旧参考脚本也实际执行并验证，编码器与141513 bytes标签配置完全一致，使用现有profile许可后manifest亦完全一致；研究parity夹具保留在临时输出、不入正式模型目录。
4. 前端 `pnpm typecheck`、1209项测试、colors/arch/i18n和有/无AI两次build通过；chunk大小旧警告保留，未因本波扩大功能。
5. `cargo test --workspace`：核心1437通过/12ignore，RAW集成7通过，桌面lib113通过/1ignore、main2通过，其它bin/doc tests通过。无AI检查通过；`cargo tree ... -i ort`提示无该包，推理依赖不在默认图中。AI特性检查与status capability测试通过；标签AI子集27、XMP子集62通过，默认无推理依赖也保留来源/禁止往返。
6. Windows有→无→有：前两次固定源码镜像，5m31s/6m47s；最后正常 `pnpm debug:win -- --ai=required` 成功，前端/RAWworker/AI原生标记/资源/许可全部核对。副本分别保留。CPU worker两张公开图512有限特征、非法协议/缺原片拒绝、EOF退出；无AI worker明确错误、stdout0、exit2，不进入GUI。首次无AI测试脚本失败源自PowerShell5.1读无BOM UTF8字面量；改为ASCII源和UTF8解码固定文字后通过，产品无修改。

机读证据 `docs/ai/tinyclip-v1/build-switch-smoke.json`；执行日志 `/tmp/raybend-registry-*.log` 与 `/tmp/model-registry-*.log`（临时日志不作为永久唯一证据）。

## 命令与 XMP

新CLI是开发/构建工具，不进应用命令面板。既有4个AI命令保留默认热键明确留空（批量/设置低频、需明确上下文），按前端+原生capability隐藏。基础标签操作不隐藏。

没有新schema、sidecar映射或文件命名变化。无AI构建继续编译有效标签投影、AI来源/文字禁止/纠错、XMP读写；现有往返单测已在默认无推理依赖的配置通过，不延期接线。

## 未验证和交付边界

没有推送、tag、上传、生成安装器；没有GUI E2E。GUI/真实照片效果、DPI、库任务与盘上XMP、安装器有AI↔基础版升级/残留清理、干净Win10/11的VC++运行库依赖仍归崔总现场确认。

本轮真正导出使用已核对的固定隔离环境和权重缓存；未在新系统额外重装完整Python依赖。脚本明确预检、固定官方依赖索引，失败不会登记生产源；首次依赖安装的完整网络行为待新环境现场验证。模型真实准确率仍沿用已记录的折中和限制，未重新训练或调阈值。

其它会话的共享修改保留，未对RayBend执行全树提交、回滚或格式化。原需求暂存已转入本规格与实现，删除 `todos/2026-10-04-ai-model-library-and-build.md`。
