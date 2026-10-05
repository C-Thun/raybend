# TinyCLIP v1：准确率优先的折中阈值与交付依据

2026-10-04 崔总定案：本期采用TinyCLIP、阈值折中并偏向减少误标，尽快进入上线前测试；SigLIP 2仅留 `memory/FUTURE.md` E1远期考查。原W1报告与失败记录保留在 `../2026-10-04/`，不重写原90%/60%门槛的结论。

## 固定配置

- checkpoint：`wkcn/TinyCLIP-ViT-40M-32-Text-19M-LAION400M`；revision `95ec8197b3f2fe7f747865c61ca556cf0768b2f7`；FP32、opset17、图像编码器158,885,258 bytes（151.53 MiB）。编码器字节未改，SHA256 `a8576f0f9916d6e92237996d41c256e1e3150db5cf6db48758bb63c87537bb24`。
- Rust ort 2.0.0-rc.13 / CPU ORT1.28.0，API27；batch1、intra2/inter1；原始位图/SOOC或RAW内嵌预览→长边384→官方中心裁切224。没有CUDA/Python/PyTorch产品依赖。
- 每类独立选阈值：校准 precision ≥85%，至少5个真阳性，在候选阈值中最大化F0.5（准确率权重大于召回）；相同时选更高precision、更严格阈值。允许一张多类与空结果，不使用全局softmax/top1。
- 固定一次正提示词修订后，人/海仍缺少有效校准阈值；最终只为这两类加入固定负描述的平均文本向量。分数为 `(cos正描述 − cos易混淆描述)/2`，其它类保留余弦。固定描述见 [prompts.json](prompts.json)，生产矩阵唯一存于 `crates/raybend/assets/ai/tinyclip-v1/classes.json`。没有网格搜提示词或训练新权重。
- 包格式2的 `thresholds_calibrated=true` 表示阈值完成校准与本期取舍审阅，**不表示全部类别达到原门槛或真实图库保证准确率**。外部可信manifest SHA256 `775a029ab0956f88acbd40dcb10e5fbbc174e3bb624c656b23923979684bbfc7`；图加载probe成功后才原子切换ready，不能让包自报可信。

## 公开样本回归

沿用W1的1,296张选样、1,294张有效数据（校准641、验证653），按作者分组，只使用明确0/1标签。每类阈值只读校准组；旧验证组此前已被查看，因此这里是作者分离的**回归评估，不是新盲测**。不能以反复评估同一组证明泛化。

| 类别 | 阈值 | 校准 P / R | 回归 P / R | 回归 TP / FP / FN | 回归正例 / 负例 |
| --- | ---: | ---: | ---: | ---: | ---: |
| person | 0.009986003 | 85.0% / 26.4% | 86.7% / 19.7% | 26 / 4 / 106 | 132 / 115 |
| bird | 0.255076438 | 94.3% / 60.0% | 91.7% / 62.3% | 33 / 3 / 20 | 53 / 63 |
| car | 0.236833632 | 90.2% / 66.1% | 86.7% / 75.0% | 39 / 6 / 13 | 52 / 83 |
| mountain | 0.225258425 | 88.2% / 78.9% | 86.7% / 69.6% | 39 / 6 / 17 | 56 / 14 |
| sea | 0.023426143 | 87.0% / 40.0% | 78.3% / 34.6% | 18 / 5 / 34 | 52 / 78 |
| forest | 0.193108961 | 88.9% / 96.0% | 90.4% / 94.0% | 47 / 5 / 3 | 50 / 6 |
| night | 0.218763351 | 92.0% / 92.0% | 91.7% / 88.0% | 44 / 4 / 6 | 50 / 7 |

P是发出的标签中正确的比例，R是真正含目标照片中找出的比例，不是模型置信概率。

**限制仍然存在**：人物明显漏标；海类仍较容易误标且漏标，Sea与Ocean定义不完全一致。山/森林/夜景的负例支持不足；未知类没有当负例。中心裁切容易漏掉边缘或小目标；不支持全尺寸RAW补算、多裁块或检测框。真实图库效果留崔总验收，不继续模型选型阻塞功能收尾。

原90%/60%门槛的字段仍保留在 [quality.json](quality.json) 作为对照，不能将其false改成“模型达标”。本期准入是崔总指定模型与折中偏好后锁定的实际配置。

## 安装与构建

首版权重**随应用提供**；设置→AI标签→安装模型，从应用资源复制到设备 `ai-models`，校验后探测真实图、原子发布ready。初始应用资源约152MiB模型数据+15MiBCPU DLL；主动安装再占一份约152MiB模型副本，完整安装器压缩大小尚未测量。保留可信官方离线目录导入。卸载只移除设备模型，不删标签/人工纠错/禁止，也不删安装器里的恢复来源。不自动扫描照片库。

模型来自独立 `model-registry`，软件 Git 只保留兼容契约与准入摘要。本机源 `ai-model-source.local.json` 被 Git 忽略；普通构建消费成品，不拉取原始权重、不导出 ONNX、不改模型 Git。未配置或资源不可用时，默认提示并构建无 AI 版本；`--ai=required` 强制输入完整，`--ai=off` 明确关闭。无 AI 仍保留已有标签、来源与禁止，隐藏识别命令/按钮，设置显示已批准的基础版状态。

在 RayBend 项目内使用：

```bash
# 有已初始化模型库（当前相邻仓库已准备好）
pnpm ai:export -- ../model-registry
# 新机器已 clone/LFS 检出，登记即可
pnpm ai:use -- ../model-registry
pnpm debug:win -- --ai=required
# 可明确构建基础版
pnpm debug:win -- --ai=off
```

未指定路径的 `pnpm ai:export` 在系统用户数据目录创建专用非 Git 模型库。显式陌生目录先用 `pnpm ai:library:init -- <路径>` 初始化。导出命令先检查工具环境、库边界、锁和 Git/LFS 条件，首次准备固定 CPU Python 工具环境；转换器唯一实现在模型库。没有 CUDA/Python/PyTorch 产品依赖。

CPU ORT1.28.0 运行库由软件构建脚本管理：现成 DLL → 已核对缓存 → 固定官方 HTTPS ZIP，两份 DLL 各自验证大小和 SHA。`pnpm ai:prepare -- --ai=required` 可独立检查并冻结输入；可用 `RAYBEND_AI_RUNTIME_DIR` 指定官方本地 DLL 目录。应用内安装仍只从随包或可信离线目录复制，不新增应用内联网下载。

旧 `export_tinyclip.py` 是研究适配入口；复用模型库转换器，再生成研究用文本向量。当前已归档矩阵、阈值与 manifest 是准入依据，重新生成摘要变化必须审阅，不自动改可信列表。完整协议见 `specs/ai-model-library-build.md`。

归档分数可重复计算（仅需Python标准库，不是产品依赖）：

```bash
python3 scripts/ai/calibrate.py --manifest docs/ai/2026-10-04/public-samples.json --scores docs/ai/tinyclip-v1/scores.jsonl --precision-target .85 --beta .5 --min-tp 5 --output /tmp/tinyclip-v1-quality.json
```

`scripts/ai/worker-smoke.ps1` 可重复验证完整桌面exe的无窗口CPU worker、真实图片编码、非法协议/缺原片返回失败、进程退出；不操作真实catalog，不替代GUI E2E。界面安装、真实库识别/暂停/取消/纠错、XMP盘上读回和性能体感仍需真机验收。

## Windows 完整桌面程序冒烟

使用 `pnpm debug:win` 构建完整桌面 exe，`pnpm check:win` 核对嵌入前端、RAW 协议7、AI worker入口、可信manifest与模型/运行库资源。通过 `worker-smoke.ps1` 启动该exe的无窗口worker：真实CPU模型加载含合同探针约1,099ms，astronaut/rocket原图预处理+编码约153/115ms，峰值working set 219,897,856 bytes（约210MiB），每图512个有限特征；错误协议和缺原片均明确失败，EOF后exit0。机器记录见 [windows-worker-smoke.json](windows-worker-smoke.json)。这是两图小样本，不是冷磁盘/大库吞吐分位或GUI验收；先前W1 Windows探针记录仍保留。

CPU ORT 官方 Windows DLL 导入 MSVCP140/140_1、VCRUNTIME140/140_1；干净 Windows 安装须同时验收 VC++ 运行库的可用性与缺失反馈。[ORT 官方安装说明](https://onnxruntime.ai/docs/install/)要求 Visual C++ 2019 runtime，并推荐最新版。当前开发机器加载成功；正式安装器尚未生成，不能据此声称干净 Win10/11 无前置组件即可运行。该依赖与 CUDA 无关。

最终共享源码在构建期间持续变化，改用2026-10-04 13:21:13（UTC+8）的临时源码快照构建并核对，不覆盖原工作区。测试副本单独保存在 `C:\rb-target\raybend-ai-test-2026-10-04\raybend-desktop.exe`，同时复制已核对的RAW worker、模型、CPU DLL与许可。快照4m26s构建通过，核对4个前端引用、RAW协议7、模型/运行库及可信摘要通过；交付时AI源文件摘要与原工作区一致。快照身份和exe摘要见 [delivery-snapshot.json](delivery-snapshot.json)。

最终测试副本再次通过worker冒烟：加载含探针1,027ms，公开两图预处理/编码151/135ms，峰值238,080,000 bytes（约227MiB），拒绝错误协议/缺原片，EOF后exit0。见 [windows-worker-smoke-final.json](windows-worker-smoke-final.json)。与首次210MiB记录分别保留，不把两次小样本测量当作稳定吞吐或内存上限。原工作区其它模块仍可变化，后续整体验收/发行应按届时源码重新统一构建。

## 独立模型库与可选构建验证（2026-10-04 续修）

相邻 `model-registry` 已初始化并以 LFS 提交编码器，标签配置作为单独 profile；固定 CPU 实际重导出摘要完全一致，PyTorch/ORT 最大绝对误差8.30e-6。指定 Git 库导入/自动提交、重复复用、默认用户数据目录实际导出和 `ai:use` 搬家登记已冒烟。`ai:use` 与普通构建不依赖制作工具/Python/Git LFS，LFS 实体已检出即可。

Windows 依次构建有AI、无AI、有AI；前两次冻结源码，最后一次正常 `pnpm debug:win -- --ai=required`，产物时间、4个嵌入前端引用、RAW协议7、原生AI标记及资源核对通过。当前可测试产物为 `C:\rb-target\raybend\debug\raybend-desktop.exe`；无AI副本为 `C:\rb-target\raybend-registry-test-off\raybend-desktop.exe`。无AI副本不含 ai-model/ai-runtime，worker 参数明确拒绝、stdout为空、exit2。`scripts/ai/worker-disabled-smoke.ps1` 可重复验证该合同。

首个有AI产物两图CPU编码约201/138ms，加载含探针1340ms，峰值237445120 bytes；仅是公开小样本冒烟，不能作为稳定吞吐/内存上限。基础版XMP来源与禁止往返、手动标签等仍通过默认无推理依赖的测试。机器证据见 [build-switch-smoke.json](build-switch-smoke.json)，实施范围与限制见 `implementations/2026-10-04_ai-model-registry-optional-build.md`。

未制作安装器、推送或发布；新系统首次 Python 依赖安装、GUI、真实库和安装器切换仍需现场确认。系统前置要求会在制作命令开始时检测并提示。
