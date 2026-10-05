# 本地标签识别：2026-10-04 可复现实验与当前决定

**当前结论：两个候选在本次配置与样本上都未满足七类准入要求，尚无可发布模型。** 因此可信模型清单保持空，生产识别入口显示“模型尚未发布”。标签来源/禁止、任务、CPU worker、模型校验和 XMP 接缝已经实现并通过开发冒烟；整个下期仍未完成。

这是一轮公开样本模型筛选，不是实际照片库效果验收。TinyCLIP 没有因为文件小就获准，SigLIP 2 也没有因为部分召回更好而直接替代它。

## 质量实验

从 Open Images validation 的**人工验证** image-level 标签中按作者分组，确定性地拆为 calibration / validation。每类每组优先选 50 正例、80 负例，保留选中照片的其它已知标签；未标注项始终是 unknown。共选中 1,296 张，1 张下载失败，1 张因嵌入 ICC 不符合当前 RGB v2/v4 输入契约而拒绝；1,294 张得到有效分数（校准 641 / 验证 653）。下载照片仅在实验临时目录，仓库不包含照片或大权重。

每类在校准组选择 precision ≥90% 的非空结果中召回最高的阈值，随后固定阈值评估验证组。通过条件为 precision ≥90%、recall ≥60%，并至少有 30 正例、50 个**明确负例**。不调验证组、不 softmax、不强制 top-1；全空输出不能算高精度。

表中 P/R 为验证 precision / recall；“—”表示无法选择满足校准门槛的非空阈值。

| 类别 | 验证正/负例 | TinyCLIP 中心裁切 P/R | TinyCLIP 全图保留 P/R | SigLIP 2 方形缩放 P/R | 结论 |
| --- | ---: | ---: | ---: | ---: | --- |
| 人 | 132 / 115 | — / 0% | 100% / 3.8% | 89.8% / 66.7% | 未通过 |
| 鸟 | 53 / 63 | 92.3% / 67.9% | 100% / 20.8% | 91.7% / 83.0% | 中心裁切 TinyCLIP、SigLIP 2 通过 |
| 车 | 52 / 83 | 86.7% / 75.0% | 81.6% / 76.9% | 87.0% / 76.9% | 未通过 |
| 山 | 56 / 14 | 89.7% / 62.5% | 100% / 44.6% | 96.0% / 42.9% | 负例不足；SigLIP 2 召回不足 |
| 海 | 52 / 78 | — | 60.0% / 5.8% | 100% / 7.7% | 未通过；概念定义有局限 |
| 森林 | 50 / 6 | 91.5% / 86.0% | 90.9% / 80.0% | 89.4% / 84.0% | 负例不足 |
| 夜景 | 50 / 7 | 91.7% / 88.0% | 88.9% / 96.0% | 91.9% / 68.0% | 负例不足 |

原始 TP/FP/FN、阈值、校准支持数和最多 20 个错误 ID/类见 [TinyCLIP 报告](tinyclip-quality.json)、[SigLIP 2 报告](siglip2-quality.json)。SigLIP 2 使用官方方形缩放；为共用报告 API，其 scores 下 CenterCrop / Fit 两列数值相同，**不代表两种 SigLIP 2 预处理实验**。

必须保留的解释边界：

- Open Images 的 Sea 不等于 Sea ∪ Ocean；Mountain 不等于 Mountain range。本轮沿用官方类别的明确标签，没有扩大它们。提示词含 ocean，因此海类结果受定义差异影响，不能外推为模型绝对不会识别海。
- 作者分组减少系列泄漏，无法保证不同作者之间不存在近重复。按类选样也不代表真实图库的类别比例。
- 负例不足不能拿 unknown 补成 negative；山、森林、夜景还需要独立、符合产品定义的标注集。没有完成人工逐张困难样本审查。
- 比较只覆盖固定 checkpoint、每类两个固定英文描述、FP32 与本次输入路径。失败结论针对这套配置，不能否定这些模型的其它用途。未做 INT8、提示词搜索、多裁块或训练。

## 模型、版本和 CPU 证据

| 项目 | TinyCLIP | SigLIP 2 |
| --- | --- | --- |
| checkpoint | wkcn/TinyCLIP-ViT-40M-32-Text-19M-LAION400M | google/siglip2-base-patch16-224 |
| 固定 revision | 95ec8197b3f2fe7f747865c61ca556cf0768b2f7 | 75de2d55ec2d0b4efc50b3e9ad70dba96a7b2fa2 |
| ONNX 文件 | 158,885,258 bytes（151.53 MiB） | 371,679,748 bytes（354.46 MiB） |
| ONNX SHA256 | a8576f0f9916d6e92237996d41c256e1e3150db5cf6db48758bb63c87537bb24 | 83770e4cc3f239846cd7ce41c549c70b5fa67533534f85b66d26081cf69298c3 |
| 输出 | image_features float32 [1,512] | image_features float32 [1,768] |
| 相同张量 PyTorch/ORT 最大误差 | 6.348e-6 | 2.354e-6 |
| WSL Rust 热推理 P50/P95（合成张量30次） | 约27 / 36 ms，随次测量见 JSON | 185 / 193 ms |
| Windows Rust CPU | 已实际运行 | 尚未实测 |

输入均为 `pixel_values` float32 `[1,3,224,224]`，ONNX opset 17。固定 `ort 2.0.0-rc.13`（API27）与官方 CPU ORT **1.28.0**，显式 CPUExecutionProvider，intra-op=2 / inter-op=1、batch=1；不依赖 CUDA。工具环境 torch 2.14.1+cpu / transformers 4.57.6 / onnx 1.23.1 / onnxruntime 1.28.0，仅供导出；产品不包含 Python、torch、tokenizer 或文本编码器。

Windows 基准机为 Intel Core Ultra 9 185H（16核/22线程），Windows 11 build 26200，内存 33,945,935,872 bytes。首轮 TinyCLIP 冷加载含一次真实图契约探测为 1.780s，热 P50/P95 为 66.7 / 118.2ms；单进程 probe 最大 WorkingSet 为 224,944,128 bytes。**该内存数不含子 worker，不能当作 worker 峰值或完整应用内存。** 两张公开照片、两种预处理的独立 worker 与直接编码器输出误差均为 0。最终worker退出复测：进程载入0.724s、热P50/P95为47.6/65.0ms；父进程峰值218,775,552 bytes、单个worker峰值219,582,464 bytes（同一小样本probe），退出码0且残留子进程0。此复测在运行过模型后进行，不代表冷磁盘加载；设备/热状态波动保留两次测量。完整应用打包/GUI/DPI/导入并发体感、空闲卸载内存、真实 RAW 预览缺失率、完整预处理和落库时延尚未实测。

Rust/Pillow resize 不是逐像素相同：[小样本差异](preprocessing-parity.json)记录输入最大差约0.060、类别余弦分数最大差0.002426。本次**实际标签校准全部通过 Rust 原始输入→长边384→224→编码器路径**，没有直接搬 Python 阈值。生产只允许512维、≤256 MiB的包校验上限，资源目标仍为≤200 MiB；SigLIP 2仅为768维对照，未偷偷扩大产品配置。

CPU DLL 实验使用 [官方 ORT 1.28.0 Windows x64 zip](https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/onnxruntime-win-x64-1.28.0.zip)，SHA256 `abef733dacbe2f571547a7150b479b5cb9cc0df22f96c24983a42cadb1b4f8bc`。DLL 按私有绝对路径加载；许可与原始 NOTICE 归档在 `legal/onnxruntime-1.28.0/`。它们尚未加入默认应用资源，模型包也未发布。

## 复现

1. 在独立实验 Python 环境安装上述版本的 CPU 工具，不改 RayBend 项目依赖。运行 `scripts/ai/export_tinyclip.py --output <tiny-dir>`。对照运行 `scripts/ai/export_siglip.py --output <siglip-dir>`，再运行 `scripts/ai/prepare_siglip_reference.py --output <siglip-dir> --tiny-reference <tiny-dir>/probe.json`。模型上游 revision 固定在脚本。
2. 按 [Open Images 官方下载说明](https://storage.googleapis.com/openimages/web/download.html)准备以下文件；精确摘要见 [measurements.json](measurements.json)。

   - `classes.csv`：[官方类别表](https://storage.googleapis.com/openimages/v5/class-descriptions.csv)。仅用于对照类别名称。
   - `labels.csv`：[v5 validation human image-level 标签](https://storage.googleapis.com/openimages/v5/validation-annotations-human-imagelabels.csv)。
   - `images.csv`：[validation 图片来源/作者/许可元数据](https://storage.googleapis.com/openimages/2018_04/validation/validation-images-with-rotation.csv)。

3. 运行 `scripts/ai/prepare_openimages.py --directory <sample-dir> --download`。脚本验证元数据摘要，用固定 seed `raybend-ai-w1-2026-10-04`、作者 hash 分组和每类支持数选择；最多5并发、每张≤8MiB。下载失败排除，保留各照片 CC BY 2.0 作者、许可与原始落地页。源照片重下载若字节漂移，须对照归档 `public-samples.json` 的摘要，不声称与本次像素完全一致。
4. 对生成 manifest 过滤无 `path` 项，保存 `<usable.json>`。编译 `cargo build -p raybend --example ai-probe --features ai-probe`；运行 `target/debug/examples/ai-probe <absolute-ORT-library> <tiny-dir> --score-list <usable.json> > <scores.jsonl>`，SigLIP 2同理。该入口不会写 catalog 或正式标签。
5. `python scripts/ai/calibrate.py --manifest <manifest.json> --scores <scores.jsonl> --output <quality.json>`。归档分数可以直接复算：`python scripts/ai/calibrate.py --manifest docs/ai/2026-10-04/public-samples.json --scores docs/ai/2026-10-04/tinyclip-scores.jsonl --output /tmp/tinyclip-quality.json`；SigLIP 2替换相应文件。单元测试：`python -m unittest discover -s scripts/ai -p 'test_*.py'`。校准无需装模型工具。
6. Windows 编译使用仓内 `scripts/lib/dav1d-win.mjs::windowsBuildEnv`，目标在 `C:\rb-target\raybend`；运行 `ai-probe.exe` 的同一参数，两个路径均为 Windows 本地绝对路径；可复用 `scripts/ai/windows_probe.ps1 -Executable <ai-probe.exe> -DataDirectory <probe-dir>` 测两张固定公开图、退出与父/子进程峰值。此处是 debug probe，不生成安装包或发布物。

原始分数、公开样本来源/许可、实验指标和处理差异均已归档，照片/权重/临时工具环境不进入 Git。数据报告的失败 ID 可回到官方元数据查看；仓库没有冒充人工复核的“典型错误截图”。

## 下一步的具体取舍（待崔总定案）

建议保留现有标签/任务/XMP底座，将下一次模型实验改为**轻量物体检测负责人/鸟/车、场景分类负责山/海/森林/夜景**，统一一个可安装模型包、串行CPU执行。先重新定义海/山等概念、补独立困难负例，再各测一个候选，不扩大成人脸或自然语言搜索。

这会突破原 W1“两个候选、一个编码器”的有界方案，需要崔总决定后再锁具体模型、许可与体积预算。另一条路线是继续单编码器，但先补产品定义一致的独立标注集重新比较；不通过降低阈值或删掉失败类别来宣称原目标完成。
