2026-10-04 13:37:07 +08:00

# 色彩管理核心攻关：复杂 ICC、输入吞吐、60MP 内存与 RAW 标度

本轮延续 `2026-10-04_color-management-float-gpu-and-persistence.md`，规格为 `specs/cm-w3-hardening.md`。没有提交、推送、发版，也没有变更界面设计。保留工作区同期 XMP/AI/预设等改动。本记录只归属下列核心修改；前一记录里的当时限制和基准保留作历史证据。

## 1. 已解决的工程问题

### 复杂 RGB 显示 ICC

保留原矩阵/TRC 快路，增加两种受验证能力，均由同一 `ScreenTransform`、wgpu renderer、设备恢复和对比路径消费：

1. **原生 mBA 阶段**：ICC B 曲线 → 矩阵/偏置 → M 曲线 → 原始非等轴 CLUT → A 曲线。支持 ICC 参数曲线 0–4 和恒等/单 gamma 曲线；XYZ 使用四面体插值，Lab 使用三线性，与本地 LCMS 实现相同。原始 8/16-bit CLUT 转 FP32 节点，避免 half 节点误差被后续陡曲线放大。展开表上限 32MiB。不能描述的配置继续尝试 PCS 路径。
2. **PCS 重采样路径**：33/65/129³ 的 LCMS PCS→设备编码表，RGBA16F；先在设备编码域插值，再做 sRGB surface 补偿。工作 Rec.2020 先映射 D50 XYZ，不把负值或 >1 工作 RGB 提前夹掉；XYZ 地址使用 sqrt 暗部密度，Lab 使用规范化 PCS。mft2 Lab 按 LCMS 实际插入的 `65280/65535` 编码阶段扩展有效域，包括 v4 头内的旧 mft2 标签。

每份配置用 NO_OPTIMIZE、相对色度、无 BPC 的 LCMS 完整输出链作为权威，独立工作域随机/暗部/边界/越界探针核验；PCS 重采样还检查每个 cell 中心。准备预算 0.75/255 编码值，逐分量检查非有限数，不让 `f32::max` 隐藏 NaN。超预算明确返回 `GpuClutAccuracy`，不放宽阈值、不偷取冲突矩阵、不改配置身份。离屏测试另包含 GPU 插值/half/目标格式误差。

原始字节哈希缓存最多 8 项、48MiB（包括失败结果，样本用 Arc）；屏幕失效时仍重新读盘，原有 generation 丢弃迟到结果、OS managed/scRGB 与应用 ICC 互斥均保留。A→B→A 或 force 刷新不重复准备同一字节配置。

**关键实证**：官方 sRGB v4 displayclass 的相对色度 B2A1 虽只有 508B，仍包含一个 2³ CLUT。整 PCS 重采样约 609ms 后不达误差门槛，不能用加密表格/放宽预算掩盖。保留原生阶段后该真实配置通过，WSL 准备约 5.18ms、Windows DX12 约 9.53–10.40ms；离屏最大偏差 1/255。Windows FXC 曾拒绝动态向量下标写入，改为等价 `select` 分量掩码后实际 DX12 编译与运行通过。

### 一次输入转换

复杂输入仍逐像素执行权威 LCMS，不使用显示近似表。准备结果按 ICC 哈希最多保留 8 份；ThreadContext 和 NO_CACHE/NO_OPTIMIZE transform 保持正确生命周期。大图最多 4 路 disjoint 输出分块，同一时刻仅一个大任务展开；等待许可发生在准备/输出分配之前，避免多任务排队时各占整图 f32 输出。RGB8 直接输入 LCMS，去掉整图扩成 RGB16；导出扁平化使用原分配。

同一进程对照 serial reference，每个 f32 完全相等（包括 131073 像素分块边界）。转换仍只在载入/源变更执行，不进滑杆/平移的逐帧路径。

### 60MP 源/参考与上传内存

无空间处理时 `WorkingPreview` 共享已有线性 f32 源，不再先复制整张中性工作图再持有 480MB half；主图/参考图若源 Arc 相同，共享 GPU texture。reference 不再调用会改写主图 lens/NR neutral key 的 `self.render`，避免每次 settled reference 把主缓存顶掉。替换空间结果前释放过期 neutral。

RGBA16F 上传按最多 8MiB 行块进行；有界 f32 RGBA scratch → half crate 批量转换 → 字节暂存复用，scalar 与 SIMD 的范围/舍入边界逐位对照。每块提交并等待完成，限制 wgpu 隐藏 staging 累积；单块 5 秒超时，错误保持到既有渲染监督/设备恢复，不能忽略错误展示半上传结果。恢复仍持有共享 f32 真相源。

这是减少重复分配的工程处理，不是把完整 60MP 降成代理。源 CPU 720MB 和 GPU half 480MB 仍需要空间。初次 Windows 测量显示 2.69 秒同步上传会阻塞渲染线程及 RenderState mutex，故没有在此收口：最终增加受控后台上传与整帧原子提交，见下节。同步诊断仍保留作同口径基线。空间算子、高质量降噪和 RAW worker IPC 仍有各自必要中间结果，未宣称它们总峰值已全部消除。

### 后台上传与完整帧提交

`render/upload.rs` 使用一个常驻后台 worker、一个可替换 latest 请求、一个完成结果槽；没有无限 channel/线程/纹理队列。每个 8MiB tile 之间检查 generation，取消/销毁不在渲染线程 join 驱动等待，旧 device 的迟到结果不可安装。活跃任务最多主图+参考两源，待替换请求也最多两源；旧完整帧在准备期间继续可画。换图、清空、恢复取消；同源 tone 新任务复用进行中的上传，避免连续拖动反复重传。

editor 将 Developed 分成准备与 Present 两阶段：后台源未齐时不写新 tone、source_size、reference_ready、decode ready 或 applied_params_rev。完成后按最新 job/reference sequence 原子提交原有整段状态。已有源的纯参数更新同步返回 ready，无额外上传/异步一帧。没有输入时 pending 状态通过有等待的 16ms 轮询完成；输入能立即打断等待。常规编辑帧的源 texture 创建/打包/逐块 poll 在 worker，提交阶段只取准备完的 texture 和创建 bind group。初始 GPU context/shader 建立、设备恢复重建已有资源仍为同步一次性路径，不能称全部 GPU 操作都异步；恢复先取消旧任务，Offscreen 的同步与异步诊断共用同一上传函数。

`color-memory deferred async` 保留旧 2MP 源，以 800×400 回读逐帧确认像素完整且不变，统计 dispatch/poll/commit。WSL 该轮总准备上传 2.616s，期间画出 64 张完整旧帧；dispatch 41.8µs、最大 poll 10.6µs、commit 699.8µs；并发 draw（含回读）median 4.47ms / p95 7.97ms。进程最大 RSS 1,382,448KiB，包含旧 2MP 源纹理/回读。运行时总资源与帧耗时仍可能受 GPU 带宽和 OS 调度影响，后台化不等于传输免费。

额外 ignored GPU 回归实际验证：第一请求被 latest 替换；准备期间旧像素不变；取消的结果不可提交；设备恢复后旧设备结果不可提交；新设备重新上传和提交成功。worker 单元覆盖连续 98 个 latest 替换只执行最终请求、活跃取消和无旧结果回流。

### RAW 的负值与冻结重放

核对本地 rawler 0.8 源码：Rescale 裁掉黑电平以下的负值，**本来没有统一裁掉 >1 高光**。adapter 新标度改为 `(sample-black)/(white-black)`，不夹负值/>1；检查黑白电平、实际重复网格配对、有限输入，复用上游 demosaic/Fuji/crop/校准。只检查黑白重复单元而非先对整图做一次取模验证；处理后尽早释放传感器数据。

新身份由旧相机矩阵 id 加固定 `rawler-0.8-unbounded-scale-v2` revision 派生；保存的旧 id 继续选择旧 Rescale，未知 id 明确拒绝。worker 协议 v7 传递冻结身份，旧 worker 握手拒绝。XMP/issue 结构、旧 hash、旧处理版本不自动升级，新 id 走已有色彩身份/缓存失效通道。复用原输入、编辑、代理和输出，不另建 RAW 色管实现。

同时修复 RAW 输入预检原先完整 `decode_working` 后只取 id、随后编辑器再次解码的问题：新增隔离 worker 的 `color-identity` 请求，使用 rawler dummy metadata 取得并校验矩阵身份，无 demosaic/整图 f32 输出/像素 IPC；原片格式/签名预检仍在。

## 2. 独立数值与真实样本

- 手工字节构造 ICC，独立于实现：v2/v4 LUT8、v2/v4 LUT16、v4 mAB 输入/mBA 输出，XYZ/Lab 共 10 组，含交叉通道 CLUT 和故意冲突的 sRGB matrix 标签。输入对照 LCMS；GPU 显示不能退回抽矩阵。另有陡峭输出曲线样本明确拒绝。
- Windows DX12 Intel Arc（driver 32.0.101.8243）和 WSL Vulkan llvmpipe：上述 10 组、内置 3 组及官方真实配置最大误差均 1/255；proof、tone、legacy、scRGB 范围/白点、设备恢复均通过。它们是数值离屏冒烟，不是屏幕颜色或 GUI E2E。
- 官方样本仅只读使用 `/tmp/raybend-cm-research/sRGB_v4_ICC_preference_displayclass.icc`，60988B，SHA256 `f54b145a18e4b12112750e672f1c79cac9347dc8403da3955e7f74a352816a21`。官网 https://registry.color.org/rgb-registry/srgbprofiles；下载来源及 libpng 登记的长度/CRC32/Adler 交叉核验在相邻 `srgb-v4-provenance.json`。没有复制进仓库，也没改 profile class。
- 真实输入样本只读使用 RawTherapee 的 `rtdata/iccprofiles/input/Panasonic DMC-G1.icc`（RGB scnr/XYZ/mft2），不分发。
- 三份真实 Panasonic DC-G9 RW2：`P1000019/P1000021/P1000096.RW2`，dummy id 均与完整当前解码 id 相等；每份固定当前 id 的再次解码逐 f32 SHA256 一致；旧 id 分别成功重放且与新结果不同。没有凭这些数值宣称 RAW 视觉正确。

2048×1536 RW2 输出数值（负值/>1 为通道计数）：

| 文件 | 当前范围 | 旧范围 | 当前/旧负通道 | 当前/旧 >1 通道 |
| --- | --- | --- | --- | --- |
| P1000019 | -0.007283747 … 2.5297275 | -0.0009234791 … 2.5297275 | 725 / 519 | 45619 / 45619 |
| P1000021 | -0.006533201 … 1.9761482 | 同范围 | 209 / 93 | 126635 / 126635 |
| P1000096 | -0.004749483 … 1.9969771 | -0.00022705883 … 1.9969771 | 49 / 8 | 69 / 69 |

P1000019 完整 5184×3888：当前范围 `[-0.045802552,2.5507474]`，旧 `[-0.01165376,2.5507474]`；负通道 43349 / 41765，>1 通道均 292723。当前 SHA256 `ac57c29ab0de842c467298ec0d975e531f0ebbdb865082018bc8f155da15b920`；旧 `1cdc15a43461cf752e5a5212c23bb305930092819d0a192b1c9afc8ae60447ba`。当前冻结重放完全相同。

## 3. 可重复资源与时间证据

O2 dev（依赖 O3），WSL，安静测量。软件 GPU 为 llvmpipe，**不能当 Windows 硬件结论**。输入基准只转换 2,000,000 RGB16 像素，同进程全量逐 f32 比较：

| 输入配置 | 首次准备 | serial LCMS | prepared 分块 | 加速 |
| --- | --- | --- | --- | --- |
| 独立合成 CLUT | 0.54ms | 280.91ms | 89.23ms | 3.15× |
| Panasonic DMC-G1 实际 mft2 | 2.03ms | 415.72ms | 143.18ms | 2.90× |

60MP 8000×7500，`/usr/bin/time -v` 独立进程实测（不是仅按对象大小估算）。packed 对照重现旧 neutral f32→half 的持有方式，**也用了新的有界 staging**，因此没有把旧无限 staging 算入收益：

| 模式 | 源准备 | frame 准备 | GPU context | 源转换+上传 | packed 常驻 | 最大 RSS |
| --- | --- | --- | --- | --- | --- | --- |
| deferred | 762.78ms | 166.65ms | 104.33ms | 1139.64ms | 0 | 1,342,396KiB |
| packed 对照 | 565.33ms | 1861.92ms | 142.74ms | 496.51ms | 480,000,000B | 1,879,080KiB |

峰值降低 536,684KiB（28.6%）；frame+上传 2358.43→1306.29ms。源准备计时有系统波动，两个模式的源生成算法相同，不能将这项差值归因于改动。此前标作约 2.54s 的诊断是 context+上传合计且仍为 scalar 打包，本轮已拆开口径。Windows WorkingSet 和硬件 draw 数据追加在后续验证段；RSS 与 Windows WorkingSet 不混算，独立进程主存也不等于 GPU 专用显存。

WSL `/mnt/c` RAW identity warm worker 200–251ms（含文件重读和 IPC）；完整 20.16MP 当前/旧解码+指纹分别 3.152/3.028s。这不是纯归一化算子计时。probe 全部重放的 `time -v` 最大 RSS 699,824KiB，是父/子进程报告的最大值，**不是父子同时合计峰值**。

重现入口：

```sh
cargo test -p raybend --lib color:: -- --nocapture
cargo test -p raybend --lib render::image::tests:: -- --nocapture
cargo test -p raybend --lib color::icc::tests::bench_two_megapixel_clut_input -- --ignored --nocapture
RAYBEND_ICC_SAMPLE='/path/to/input.icc' cargo test -p raybend --lib color::icc::tests::bench_two_megapixel_clut_input -- --ignored --nocapture
cargo run -p raybend --example color-offscreen -- /path/to/display.icc
cargo run -p raybend --example color-draw-cost -- /path/to/display.icc
cargo build -p raybend --example color-memory --example color-raw-probe --bin raybend-raw-worker
/usr/bin/time -v target/debug/examples/color-memory deferred gpu
/usr/bin/time -v target/debug/examples/color-memory packed gpu
/usr/bin/time -v target/debug/examples/color-memory deferred async
target/debug/examples/color-raw-probe /path/to/photo.RW2 2048 target/debug/raybend-raw-worker
target/debug/examples/color-raw-probe /path/to/photo.RW2 0 target/debug/raybend-raw-worker
pnpm exec node scripts/check-color-win.mjs /path/to/display.icc
```

不把 60MP 或真实文件放进普通单元测试；2MP 性能和 GPU 大回归为 ignored/独立 example。Windows 脚本复用既有 dav1d Windows 构建环境和同一个 renderer，不另建渲染实现。

## 4. 文件、契约与交接

核心文件：`color/{display,display_clut,display_native,icc_fixtures,lcms_input,icc,matrix_trc,input}.rs`；`render/{color_transform,gpu,image,working_preview,scene,upload}.rs` 与 `spike.wgsl`；`develop/working.rs`；`raw/{backend,rawler_backend,worker}.rs`；`export/output.rs` 的冻结身份传递；`src-tauri/src/editor.rs` 的完整帧提交；四份 `examples/color-*.rs` 与 `scripts/check-color-win.mjs`。规格同步 `cm-w2.md`、`cm-w3.md`、`cm-w3-hardening.md`。

无新界面可触发功能；本轮是既有命令的内部实现和开发诊断，不新增命令面板条目/defaultKey。XMP/issue 形状不变；新 RAW 数值由新的输入身份明确区分，旧冻结身份保留旧算法，显示 ICC 从不进入照片缓存或成片。

仍需崔总实际验收：真实 RAW 暗部/高光视觉、真实显示颜色、多屏/DPI/ACM 切换、全窗口交互与切图体感。复杂 CLUT **软打样**仍不支持；本轮完成的是受验证的复杂 CLUT **显示**。原生 mBA reader 是明确子集，任意不兼容 ICC 可以被拒绝；没有宣称覆盖所有有效 ICC。

可留 Sol 的常规收尾：设置页当前显示原始 OS snapshot，需与 editor 实际成功/回退状态统一；本地化 `panels.tsx` 当前裸显的 displayColor 字符串，将状态枚举/来源与设置页复用；修正 zh/en `settings.display.systemManagedDetail` 尚称“呈现协商仍在接入”的过时说明。`settings.coming` 文案本身已正确，无需因 key 名字修改。它们是界面状态接线和文案，不改变本轮色彩算法或把未支持软打样标成支持。

核心测试日志：`/tmp/raybend-astra-core.log`（较早全量 1430 passed / 11 ignored）；`/tmp/raybend-astra-color-final.log`（新增色彩 59 passed / 3 ignored）；`/tmp/raybend-astra-image-tests.log`（tile/half 7 passed）。数值/性能原日志统一前缀 `/tmp/raybend-astra-*`，Windows父审查日志 `/tmp/raybend-parent-win-*`。最终全量门结果见本文第 7 节。

## 5. Windows DX12 工程验证

Intel Arc IntegratedGpu，driver 32.0.101.8243，O2 dev。2MP `color-draw-cost` 复用目标，每条为 draw+submit+完成等待，不含回读、源上传或配置准备；**不是纯 GPU timestamp，也不是整窗 E2E**。两轮均保存，不能挑低值当最终闸门：

| 组合 | 第一轮 median / p95 ms | 第二轮 median / p95 ms |
| --- | --- | --- |
| legacy sRGB8 | 2.262 / 3.342 | 3.019 / 13.447 |
| legacy sRGB8+P3 ICC | 2.932 / 25.176 | 3.631 / 12.554 |
| legacy sRGB8+Adobe RGB TRC | 2.154 / 7.160 | 4.010 / 83.838 |
| V2 source only | 2.170 / 6.864 | 3.122 / 9.716 |
| V2 global tone | 3.055 / 7.667 | 4.132 / 8.323 |
| V2 tone+ICC | 2.701 / 8.179 | 4.360 / 11.720 |
| V2 tone+Adobe RGB TRC | 3.178 / 38.938 | 4.603 / 35.862 |
| V2 tone+ICC+proof warning | 2.637 / 4.789 | 3.817 / 9.626 |
| V2 tone+chroma+ICC+proof warning | 3.512 / 5.150 | 4.335 / 9.795 |
| V2 complex tone+CLUT Lab=false | 5.376 / 9.029 | 7.577 / 46.889 |
| V2 complex tone+CLUT Lab=false+proof warning | 5.258 / 24.034 | 6.663 / 11.690 |
| V2 complex tone+CLUT Lab=true | 4.110 / 8.611 | 5.629 / 52.501 |
| V2 complex tone+CLUT Lab=true+proof warning | 4.164 / 8.950 | 5.002 / 10.249 |
| V2 complex tone+external ICC | 4.589 / 8.387 | 5.683 / 10.307 |
| V2 complex tone+external ICC+proof warning | 4.616 / 26.140 | 5.764 / 59.092 |
| V2 tone+scRGB FP16 | 3.145 / 3.874 | 3.894 / 8.873 |

第二轮同时观察到既有 tsserver/Vite/Windows 后台负载；旧 sRGB+Adobe 也出现 83.84ms p95，不能把所有尾抖动归于新 CLUT，也不能据此宣称性能已经稳定过闸。官方 native-mBA 准备 10.40 / 10.81ms、上传 2.60 / 2.56ms。该表的 proof 使用现有矩阵/TRC RGB 软打样；不代表复杂 CLUT 软打样。

60MP Windows 独立进程高水位（PowerShell 每 10ms 取 `PeakWorkingSet64`/`PeakPagedMemorySize64`，前者不等于专用 GPU 显存）：

| 模式 | frame prepare | source upload | context | PeakWorkingSet B | PeakPagefile B |
| --- | --- | --- | --- | --- | --- |
| 同步 deferred 诊断 | 289.82ms | 2688.84ms | 5050.18ms | 1,810,288,640 | 1,807,085,568 |
| packed 对照 | 2677.23ms | 820.11ms | 2940.25ms | 2,249,056,256 | 2,265,497,600 |
| 最终异步 deferred，保留旧 2MP 图 | 245.53ms | 3103.79ms 后台 | 1485.85ms | 1,814,036,480 | 1,828,892,672 |

同步 deferred 比 packed 的主存高水位低约 19.5%，但 2.69s 渲染线程阻塞仍不可接受，因此采用最终后台路径。异步期间 **63 张完整旧帧**逐次回读相等；dispatch 15.4µs、poll 最大 44.6µs、commit 2.1412ms。800×400 含回读的并发 draw median 28.36ms / p95 46.46ms，不能拿来替代上面的无回读 2MP draw 测试；已给探针增加上传前同口径基线用于区分回读/系统等待与上传干扰。初始化与恢复同步成本另列，未被藏进“上传微秒级”的表述。


同口径 idle 基线补测（最后一轮，`/tmp/raybend-parent-win-memory-deferred-async.log`；首轮另存 `-async-first.log`）：源准备 611.76ms、frame 304.71ms、context 2264.44ms。800×400 **含回读**的上传前 median/p95 为 14.71/29.23ms；后台上传中为 16.08/44.98ms，median 增加 1.37ms。3.434s 后台上传期间画出 **75 张完整旧帧**；dispatch 17.5µs、最大 poll 53.7µs、commit 3.6863ms；PeakWorkingSet 1,813,934,080B / PeakPagefile 1,828,683,776B。已直接证明多秒全阻塞消除，但尾延迟和实际 GUI 体感仍未作通过承诺。

最终代码静态检查 `git diff --check` 通过；后台 uploader 1 个并发单元和 1 个 ignored GPU latest/cancel/recovery 冒烟通过。`cargo check -p raybend-desktop` 通过，前端 typecheck、1203/1203 tests、colors/arch/i18n 全通过。最终 workspace 测试和 Windows desktop+worker 产物检查均已通过，结果见第 7 节；本轮没有提交/发布。


## 6. Sol 常规交接：显示状态与本地化

核心算法已冻结，Sol 的范围只限现有界面的状态接线与文案：

1. `src-tauri/src/editor.rs::RenderState.display_color` 当前为裸字符串；整理成有明确状态/原因/来源的结构，区分实际 ICC 生效、OS 管理 sRGB/scRGB、准确回退与不可用。原始 `color_display_snapshot` 保留 OS 事实，不当作“实际 GPU 已采用”的结论。同步 Rust/TS DTO 与现有契约测试。
2. `src/features/editor/panels.tsx` 当前直接渲染 `renderState.displayColor`，会裸显 `icc` / `system-managed-scRGB` 或英文异常；与 `GlobalSettingsDialog` 复用一份状态展示/映射，使用实际准备结果并保持 ICC/OS 状态来源清楚。复用已有 color/UI 组件与 i18n，不复制两份判定逻辑。
3. 更新 zh/en `settings.display.systemManagedDetail` 的旧“标准空间呈现协商仍在接入”说明。`settings.coming` 只是旧 key 名，现有文案已正确，**不要因名字把它误改回骨架提示**。
4. 复用已有 Pencil 稿与同名说明；若改变布局或交互，遵守 `AGENTS.md` 的设计先行纪律。这份交接没有授权额外界面重设计，不扩展到复杂 CLUT 软打样。

本交接是已有功能的状态表达，没有新增独立命令/快捷键；显示状态不进入 XMP/issue/成片。算法、RAW revision、GPU 上传与缓存策略已由本轮实现和验证，常规交接不承担新的色彩技术攻关。


## 7. 最终工作区质量门与 Windows 配套产物

父审查在算法与源码冻结后执行：

- `cargo test --workspace`：核心库 1439 passed / 12 ignored，worker 集成 7 passed，桌面库 108 passed / 1 ignored，桌面主程序 2 passed，文档 2 passed；合计 1558 passed / 13 ignored，零失败。ignored 的本轮相关大图、GPU、输入性能入口已按本文前述独立执行，未宣称所有历史 ignored 均执行。核心测试运行 21.64s，首次相关编译 3m43s；不把编译时间当测试执行时间。日志 `/tmp/raybend-parent-workspace-tests.log`。
- `cargo check --workspace` 通过，日志 `/tmp/raybend-parent-workspace-check.log`。
- 前端 `pnpm typecheck`、`pnpm test`（1203/1203）、`pnpm lint:colors` / `lint:arch` / `lint:i18n` 全通过；收口核对 `src/`、`scripts/` 没有这些门之后的 TS/TSX/MJS 修改。
- 最终 Windows DX12 `color-offscreen` 再次通过：内置/独立/真实 ICC、tone、proof、legacy、scRGB 与设备恢复。日志 `/tmp/raybend-parent-win-offscreen-final-run.log`。数值离屏仍不等于 GUI/实际屏幕验收。
- `pnpm debug:win` 完成前端构建、Windows `custom-protocol` 主程序与 worker 配套构建，并由脚本执行产物核对；4 个前端引用资源全部命中，worker 含 `raybend-worker-proto-v7`。产物 `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`，最终主程序时间 `2026-10-04T05:34:40.745Z`；日志 `/tmp/raybend-parent-debug-win.log`。只做 debug 构建，没有 release、提交、推送或打开用户图库进行 GUI 交互。
- 最终 Windows 原生路径 `C:\src\tmp\pic\P1000019.RW2` 的只读 worker probe 通过。metadata identity 含首次 worker 启动 1.190s、已启动 worker 50.69ms；2048×1536 当前解码+指纹 1.298s，旧路径 1.187s。当前范围/负值/>1 计数与前述 WSL 相同；当前指纹 `695d06fc19ce78a40eb513acfb5602f229f124544a79e15864ecb8fa7ba10def`，旧指纹 `25f90ce58286eed9f51b7c9f2fb084486623a7daad2033ddfdebeee8f193fd86`，当前冻结重放逐值完全相同。日志 `/tmp/raybend-parent-win-raw-probe.log`。这一测量包含文件读取和 IPC，不冒称纯算子耗时。
- 最终 `git diff --check` 通过。已有 store/develop 测试的 unused_mut 与前端 chunk size 提示不属于本轮核心失败，没有为消除提示改动并行模块。

本轮核心工程交付和 Agent 冒烟完成；Sol 常规界面收尾见第 6 节。整窗尾延迟、实际 RAW/显示色彩及多屏/DPI/ACM 仍等待崔总统一验收，未据这些数字自动启用旧照片迁移。
