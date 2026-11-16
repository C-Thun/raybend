完成时间：2026-10-04T13:29:01+08:00（Asia/Shanghai）

# 相片整理下期：TinyCLIP 定案与默认 CPU 交付

本期代码与 Agent 冒烟收口，可进入上线前真机测试；真实图库/GUI/DPI、盘上 XMP、安装器与性能体感尚未验收。崔总本轮指定 TinyCLIP、阈值折中且偏向减少误标；SigLIP 2 仅作为 FUTURE E1 远期考查。没有继续模型选型、额外类别或分割功能，没有生成安装器、发版、推送或上传。

## 改动范围与复用

- 模型/算法：`crates/raybend/src/ai/{scoring,pack,result,registry}.rs`，`assets/ai/tinyclip-v1/`，`scripts/ai/{export_tinyclip,prepare_tinyclip_reference,calibrate,test_calibrate,build_tinyclip_pack}.py`。唯一图像编码器/预处理/评分器供实验与产品使用，新增人/海负描述适配，不另写图片算法。
- 默认 CPU/设置：两处 Cargo features、`src-tauri/src/photo_ai.rs`、`tauri.conf.json`、`src/api/photo-ai.ts`/DTO、`AiModelSettings.tsx`/`AiDialog.tsx`及中英文。复用获批 `main.pen s1SP3g`、设置外壳、Button 与目录选择；在 Pencil/配对说明更新模型大小/安装/损坏状态后接代码。
- 资源/分发：`scripts/lib/ai-assets.mjs`及小 fixture 测试、`scripts/ai/prepare-assets.mjs`、debug/release/check-win 脚本、资源元数据/许可证、许可生成器与生成清单；正式发布仍由崔总执行。
- 证据/状态：`docs/ai/tinyclip-v1/`归档提示词、分数、逐类报告与 Windows worker 日志摘要；phase2/W1/W3/总规格、PLAN/FINISHED/FUTURE、design/main.md、根许可与 legal/README 同步当前口径。早期未过门槛的实验报告保留。
- 既有来源/文字禁止/撤销、任务/稳定 UID/挂载租约、标签投影/筛选/累积自动桶和 sidecar 接线继续复用前阶段实现。没有重建词典、任务平台、归集器、XMP writer 或全局设置。

共享工作区还有色彩/XMP/导出等会话的改动；本轮仅收敛上述 AI 落点，未覆盖、回退其它模块。Windows 源码一致性闸门在并行修改期间拦下旧 RAW worker，保留严格检查，没有放宽时间戳/协议条件。多次可变源码构建仍遇到变更，按 AGENTS 的绕过纪律改用临时源码快照完成最终构建核对；原工作区保留。快照相对后续其它会话可能落后，风险明确记录，不作为最新整库发行版。

## 模型与阈值

采用 `wkcn/TinyCLIP-ViT-40M-32-Text-19M-LAION400M`，revision `95ec8197b3f2fe7f747865c61ca556cf0768b2f7`。FP32图像编码器158,885,258 bytes（151.53 MiB，界面152MiB），opset17、224×224、512维；权重字节不变。ort 2.0.0-rc.13 / ORT1.28.0 CPU（API27），batch1、intra2/inter1；不分发文本编码器、tokenizer、Python、PyTorch、CUDA。

每类独立取校准 precision 目标≥85%、至少5个真阳性支持、最大化 F0.5，相同时偏高 precision/严格阈值；允许多类和空结果，不做 softmax/top1。限定一次正提示词修订后人/海仍缺少有效阈值，最终为这两类加入固定易混淆负描述的半余弦差；其它类保留余弦。规范文字仍复用既有 clean/fold规则，分数不是概率。

1,294有效公开样本，校准641、回归653，阈值只读校准组。旧验证集此前被查看，这次是作者分离的回归，不是新的盲测。回归 P/R：人物86.7%/19.7%、鸟91.7%/62.3%、车86.7%/75.0%、山86.7%/69.6%、海78.3%/34.6%、森林90.4%/94.0%、夜景91.7%/88.0%。**人物漏标、海类误标/漏标仍明显；山/森林/夜景困难负例不足，中心裁切漏边缘/小主体。**不能以校准85%宣称真实图库保证，不能将原90%/60%字段false改成达标。

包格式2的 `thresholds_calibrated`只表示逐类阈值已经校准与本期取舍审阅；固定可信 manifest 为 `775a029ab0956f88acbd40dcb10e5fbbc174e3bb624c656b23923979684bbfc7`。生产要求完整可信包与实际图加载探针，未知/未校准包拒绝。SigLIP 2约354MiB及原实验仅留 FUTURE E1，不装入本期。

## 安装、异常与资源

首版模型源包与CPU运行库随应用私有资源提供，设置→AI标签→安装模型时主动复制到设备 `ai-models/<manifest>/`，可信文件校验和真实图探测成功后原子切换 ready-v1.json。保留可信离线目录导入，安装/卸载受同一模型操作锁与活动任务保护。失败保持旧ready；同版本损坏可重新安装，可信但损坏副本可卸载，人工/AI成果/禁止均保留。

约152MiB源包+15MiB CPU DLL 随应用，主动安装再占约152MiB模型副本；完整安装器压缩体积未测。大 ONNX/DLL 忽略Git，元数据/许可证入Git。prepare-assets统一校验摘要、普通文件/白名单、有界声明与原子资源修复；debug及人工release构建前预检；check:win核对应用入口/可信manifest/资源，RAW protocol7检查保持原样。没有假下载URL；在线下载是后续分发优化，先由崔总发布固定制品，首版离线使用不受下载站点阻塞。

CPU ORT原生DLL导入VC++140系列运行库，官方要求Visual C++2019 runtime并推荐最新版；当前机器加载成功，干净 Win10/11 安装尚未验证运行库保障与缺失反馈。依据 https://onnxruntime.ai/docs/install/ 。真实满盘没有模拟；复制失败有明确错误与暂存清理、旧ready保护，提前空间提示留后续分发优化。

本轮校验曾抓到制作脚本 `CenterCrop` 与Rust serde `center_crop`不匹配，已修正并重新冻结可信摘要；损坏同版本修复/卸载各有小数据回归。Windows校验新增AI入口最初误读独立RAW二进制，已改为检查主程序并实际跑通。UI冒烟一次在开发服务器未运行时失败，确认服务器200后复跑通过，不过滤JS模块404或放宽断言。PowerShell UNC脚本被RemoteSigned拒绝后复制到本地测试目录运行，没有改全局执行策略。

## 命令与 XMP

四个既有AI命令（识别、重新识别、任务、模型设置）继续在统一catalog登记，`defaultKey`明确留空：低频批量/设置动作，不占用浏览/编辑热键。安装/修复/卸载属于模型设置内状态动作，暂停/取消/重试属于具体任务；不另外占全局快捷键。复用既有热键格式与标签上下文命令。

已按最新 `specs/xmp-sidecar.md` 评估本轮影响：负描述分数仍在[-1,1]，不改变标签/屏蔽/结果身份存储及 `rb:tagState` JSON1格式；有效去重文字仍写 `dc:subject`，来源/文字禁止走既有自有扩展。安装/卸载不清成果、不需批量写XMP；识别/纠错沿用现有事件与sidecar队列。标准关键词、来源禁止往返、sidecar AI不转manual、未知版本/写失败回归通过。真实库磁盘上的XMP写出/重新导入仍需验收，没有扩展跨家导入或另建writer。

## 已验证（Agent 冒烟）

- `cargo test -p raybend --lib --features ai-probe`：1437 passed，0 failed，11 ignored，18.59s。
- `cargo test -p raybend-desktop --lib`（默认CPU）：108 passed，0 failed，1 ignored，0.25s。
- RAW独立worker回归7 passed，2.11s；`cargo check --workspace`通过。
- `pnpm test`：1203 passed，0 failed，5.27s；typecheck、colors/arch/i18n均通过。
- 资源/发布编排fixture：11 passed；Python标准库校准器3 passed；归档分数再算报告逐字节一致。
- `pnpm smoke:ui`：独立CDP端口9437，problems=[]。可重复脚本冒烟，不判断视觉或真实库正确性。
- 前端build、Windows debug构建与最终产物核对：见下方最终制品记录。
- `scripts/ai/worker-smoke.ps1`：完整桌面exe以无窗口CPU worker启动，私有DLL/固定模型真实加载，两图512有限特征；非法协议/缺原片明确失败，关闭输入后exit0，无GUI/真实catalog操作。第一次小样本加载含合同探针约1099ms、两图153/115ms、峰值219,897,856 bytes（约210MiB）；不是冷磁盘或大库P95。机器 Core Ultra9 185H、16核/22线程、约32GiB、Windows版本10.0.26200。

## 未经崔总真机验收

GUI安装/修复/卸载与任务/来源纠错、真实图库七类效果、真实RAW缺预览比例、盘上XMP读回、长任务/暂停取消/拔插恢复、双DPI/性能体感/极长Windows路径，以及干净Win10/11安装与VC++运行库可用性均未验收。不能将“worker正常退出”当作全部功能正确。

后续测试优先验证：设置主动安装 → 明确少量照片识别 → 来源查看/禁止/手动保留 → 重跑与找片/自动桶 → XMP新导入读回。人物漏标与海类误标需要重点观察，保留失败样例供后续修正。本轮环境未提供pi todo工具；没有另造任务状态文件，验收边界留规格与本记录。

## 最终测试制品

- 源码冻结于2026-10-04 13:21:13（UTC+8），临时目录 `/tmp/raybend-ai-delivery-snapshot`；使用原debug流程的同一Cargo参数（桌面+core、custom-protocol）和固定资源，依赖构建缓存复用。前端先前build5.91s；快照Windows构建4m26s，资源/源码/前端/RAW协议7核对成功。冻结避免并行修改污染本轮检查，不改写共享源码，不放宽闸门。
- 独立测试副本 `C:\rb-target\raybend-ai-test-2026-10-04\raybend-desktop.exe`，62,641,664 bytes，SHA256 `713984e3c053cf84ef80216aef9cfe809b6e632c6ff558a1a7ea6d66b4627665`；RAW worker14,866,944 bytes，SHA256 `a05dae9b035630b7d5214d3cf6331289cfcdc4b8aa55b1aac349c651ec3a5202`。同目录有模型、CPU DLL和许可，复制后重新核对成功。交付时AI源文件摘要与共享工作区一致。
- 测试副本再次CPU worker冒烟成功：加载含探针1,027ms，两图151/135ms，峰值238,080,000 bytes（约227MiB）；512维有限输出，错误协议/缺原片拒绝，EOF后exit0。最终机器记录 `docs/ai/tinyclip-v1/windows-worker-smoke-final.json`，源码/制品摘要 `delivery-snapshot.json`；首次记录保留，不以两图测试承诺性能上限。
- 未生成安装器或正式发行物。后续其它模块仍会继续改变，整体验收/发行必须按届时源码统一构建；本副本只用于本轮可追溯的Windows测试。
