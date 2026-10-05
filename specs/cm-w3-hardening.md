# CM-W3 核心攻关：CLUT、输入吞吐与大图内存

2026-10-04。延续 `cm-w3.md`；保持旧版处理、XMP 与 issue 契约。没有新 UI/命令，不增加依赖，不提交/推送/发版。

## 方案与边界

- 传统 SDR 复杂 RGB 显示由 LCMS NO_OPTIMIZE、相对色度生成 PCS→设备编码三维表，保留 Rec.2020→D50 XYZ 矩阵。XYZ/Lab PCS 采用各自规范域；XYZ 暗部使用 sqrt 地址；不在工作 RGB 上提前夹到 0..1。矩阵/TRC 原快路保留。
- GPU 表仅作为显示近似，配置级独立探针验证误差（含 half 量化、暗部、域边界与域外），超预算拒绝并保留准确原因。输入与导出真相仍使用 LCMS；不拿显示表做编辑输入。
- 复杂输入复用经过验证的 LCMS transform，有界并行、原位输出分块，RGB8 不产生整图 RGB16 副本。16-bit/f32 精度与 NO_OPTIMIZE 参考一致。
- 大图在既有 renderer 中减少中间工作图、参考抢占、过期 half 副本与上传暂存。保留恢复所需数据；重测试使用 ignored 入口，不把 60MP 纳入常规单元。
- RAW 先查 upstream 的实际剪裁位置，在 adapter 接管黑/白电平归一化，复用原去马赛克/裁切/校准；工程数值与真实视觉分别记录。

## 验收依据

独立手工 ICC v2/v4 LUT8/LUT16/mAB/mBA（输入/输出标签均有）；LCMS参考、GPU离屏、真实本地输入ICC、60MP合成峰值和少量真实RAW只读诊断。Windows DX12 与 WSL软件GPU分别计时；任何离屏结果不称整窗E2E。

## 最终技术补充

原 PCS 全表重采样不能可靠跨越高曲率/裁剪面。实际官方 sRGB v4 displayclass 含 mBA B→矩阵→M→2³ CLUT→A；现加受限原生阶段路径：参数曲线0–4和恒等/gamma曲线、可选矩阵/偏置、原始非等轴RGB CLUT，FP32节点，XYZ四面体/Lab三线性。每份配置仍用 LCMS 完整变换独立核验，不从名称/标记猜测兼容；无法描述/不达预算才试PCS烘焙或明确拒绝。原生表32MiB上限；准备缓存按原始ICC哈希，最多8项/48MiB，失败也记住，磁盘仍每次重新读。

mft2 Lab 的有效域按 LCMS `65280/65535` 编码校正，不能把L上限简单设100；所有工作RGB先映射PCS，矩阵前不裁域。Baked表33/65/129³，XYZ sqrt暗部地址；独立工作域暗部/随机/边界探针以及每个表格中心验证，GPU/CPU总误差另测。相对色度/无BPC与现有契约一致；复杂CLUT软打样不在这里假称支持。

RAW新标度身份由旧matrix id加固定revision派生；固化旧id仍走原Rescale，未知id拒绝。worker v7传入固化id；XMP形状不变、schema1/2与旧issue不升级。新色彩自动解析走隔离worker的dummy metadata identity请求，避免整图显影只取id。真正显影仍在worker。

60MP全局路径共享f32参考源，按8MiB半精度行块上传，half crate批量转换复用scratch；不常驻整图half。上传阶段有5秒单块超时，错误保留到原有监督/设备恢复处理，不把半上传结果显示为成功。全图纹理仍需GPU空间，空间算子/高质量降噪另有必要结果，实际峰值和阻塞按平台量化。

同步上传实测仍阻塞 Windows 渲染线程约 2.69 秒，因此最终采用同一有界上传函数的单后台 worker。一个 active + 一个可替换 latest，请求内至多主图/参考两源；分块取消、无析构 join、旧 device 结果拒绝。editor 在主/参考资源齐全且 job/reference sequence 仍匹配后才提交 tone/尺寸/ready 状态；纯 tone 更新复用源并立即通过，同源进行中上传不随 slider 任务重启。pending 以 16ms 有等待轮询完成，旧完整帧可响应输入持续绘制；换照片/clear/recovery 取消迟到结果。离屏并发探针验证旧像素完整、latest/cancel/recovery，真实 GUI 体感仍待验收。
