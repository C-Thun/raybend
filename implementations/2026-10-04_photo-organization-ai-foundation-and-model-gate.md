完成时间：2026-10-04T05:30:21+08:00

# 相片整理下期：数据/任务/UI接线与模型准入结果

**当前状态：基础接线已实现，整个下期尚未完成。** TinyCLIP / SigLIP 2在本次固定配置与公开标注验证中都未满足七类整体门槛，可信生产模型清单保持空；默认应用未启用ORT，也不包含CPU DLL或权重。模型下一路线已在会话向崔总提出，尚未定案。不能把本报告理解为“功能完整、仅待点一次GUI”。

## 授权与改动范围

崔总授权按下期方案实施，同意先用公开样本验证，并确认 browse.pen hqlQz/xaQdZ、main.pen s1SP3g 三张AI稿后继续实现。使用已读 pencil-design / frontend-design 技能；界面复用现有Dialog、Button、TagDialog、GlobalSettingsDialog和令牌。工作区同期有色彩/XMP/export开发，保留其实现与变更；收口时唯一附带重构是将现有ProfileSelect提升为 `components/ui/ProfileSelect.tsx` 并更新editor/export/settings三个引用，解决editor→color跨功能依赖，不复制组件、不改色彩算法；UI只依赖自身展示字段接口，通过结构类型适配现有API数据。色彩命令新增测试用了当前TS lib未提供的Array.at，改回末项下标，测试语义不变。没有push、tag、release、上传或整库重置。

新增核心 `crates/raybend/src/ai/`（输入/预处理/评分/结果/任务/范围/提交屏障/调度/运行库/独立worker/包校验/注册表）及 `store/photo_tags.rs`。接缝涉及 `store/{assets,marking,organization,query,tags,rebuild,migration}.rs`、`xmp/{packet,adopt,mapping,mod}.rs`、`export/metadata.rs`。原有 RAW 进程监督抽为 `worker_process.rs`，RAW仍走自己的协议6，AI协议1，不写第二份监督器。

正式迁移：catalog_0015 masks/results/attempts/有效投影与事件；catalog_0017 concept词表；app_0011 jobs唯一索引。同期catalog_0016/app_0010属于色彩主线，迁移编号协调后保留，全部走现有快照/事务/版本闸门。没有启动临时ALTER或新迁移通道。

Tauri薄壳 `src-tauri/src/photo_ai.rs` 复用DbState、BrowseState库任务租约、事件与sidecar队列；lib/main/db/browse/organization/repo只接IPC、退出和已有存储适配。`photo-ai-runtime`非默认特性启用核心`ai-probe`，精确optional ort/ort-sys 2.0.0-rc.13，ORT CPU1.28.0/API27。

前端涉及 API photo-ai/organization/types/DTO契约、AiDialog、AiModelSettings、TagDialog与tag-draft、BrowseToolbar、App、GlobalSettingsDialog、命令注册与中英文。四个动作“识别/重新识别/任务/模型设置”已登记；**defaultKey明确留空**（低频批量/设置、不抢浏览编辑键位）。禁止/保留留在既有标签编辑命令的上下文；暂停/取消/重试是具体任务对象动作，不增加无上下文全局命令。

证据/工具：`scripts/ai/{export_tinyclip,export_siglip,prepare_siglip_reference,prepare_openimages,calibrate,check_preprocessing,test_calibrate}.py`、`windows_probe.ps1`、`examples/ai-probe.rs`；`docs/ai/2026-10-04/`归档来源/许可/摘要/分数/报告，不含原照片或大权重。ORT实际LICENSE与第三方NOTICE原文归档`legal/onnxruntime-1.28.0/`并登记根许可表。

## 关键行为与取舍

- 唯一有效集合 `(manual ∪ valid_ai) − masks`，目录不继承到照片。所有文字/数字导航、筛选、桶、标记快照、导出与XMP读取统一view；未知旧数字ID拒绝丢关键词。手动同名解除禁止；移除手动保留有效AI；禁止覆盖所有同规范化文字来源。草稿/撤销只改manual/masks，不复制旧AI结果，实际TagDelta按前后有效投影计算。
- 成功（包括空结果）只替换AI。失败、取消保留上次仍有效成果。原片来源变化使旧AI无效，手动/禁止保留；路径、latest调色、issue、图片缓存不进入原始来源身份。源版本含稳定文件身份/角色/大小/mtime；提交额外重读高精度磁盘signature和UID/token。原片删除/UID复用计跳过；无原片/RAW预览拒绝假空成功。
- 原位图优先、RAW仅内嵌JPEG。worker/probe共用原片→已有ICC/EXIF→长边384→224预处理，像素不进WebView。位图256MiB、一亿像素、极端裁切中间16M像素、IPC64KiB，分数有限且逐类阈值，不softmax/top-1。每类首次文字映射持久化，改语言不重命名旧标签。
- 每库一个WAL读快照、asset_id keyset500页冻结UID/上界，完成准备才领；跨库不是全局同刻快照。**准备中断明确失败并取消半成品，重选范围**，不恢复半份范围。完整任务重开默认暂停。失败30秒退避/最多3次，离线等待需明确继续；没有无限扫描。
- 串行一张、预取0、CPU intra-op2/inter-op1。一张读原片/推理持有现有库任务租约；提交与取消由短Coordinator屏障串行，推理不占数据库写锁。catalog成功后队列确认前崩溃，重放按成功身份跳过。前台编辑/导入/实际导出停止下一张领取（目前按flow粗粒度让步）；空闲30秒卸载，进程退出同步杀worker，不等待30秒超时。
- 模型只收固定数据白名单/外部可信manifest摘要；暂存校验+真实ONNX图探测成功后才原子ready，安装失败保留旧ready。离线官方**解包目录**接口已接；没有HTTP下载器、正式URL或任意模型导入。卸载在途/准备任务受同一操作锁保护，保留数据库成果；暂停后卸载会令后续继续需要重新安装可用模型。
- XMP同次接线：dc:subject仅有效文字，rb:tagState JSON格式1保留来源/禁止/证据身份；自家扩展优先，不把AI抄成manual。自家恢复origin=sidecar并绑定当前已登记原片，缺原片无效；不伪装本机推理，也不承诺跨机器内容摘要验证。未知/非法扩展保护文件，写失败仍走原队列反馈，不回滚DB。AI提交/失效/人工纠错复用组织事件与sidecar触发，自动桶仍累积，禁止不把旧成员自动弹出，移出排除持续生效。

## 模型结果与不能启用的原因

详细报告与可复现数据：`docs/ai/2026-10-04/README.md`。固定checkpoint/revision、FP32/opset17、每类两条英文描述。Open Images按作者分校准/验证，1296选中、1295下载、1294有效（1张ICC拒绝），641/653；脚本已复现全部选中ID、标签/分组和下载图片摘要。unknown不当负例。

TinyCLIP151.53MiB：中心裁切鸟P92.3%/R67.9%通过；人召回0%、车P86.7%未通过；全图方案未改善到整体可用。SigLIP 2 354.46MiB：鸟P91.7%/R83.0%通过，人P89.8%/R66.7%、车P87.0%/R76.9%、海R7.7%未通过；山/森林/夜景明确负例分别14/6/7，证据不足。SigLIP超原200MiB目标，仅768维probe，未塞进生产512维/256MiB校验上限。

Sea/Ocean与Mountain/range概念有差异，困难负例未逐张人工审查；这些结论只约束本次配置，不能声称模型绝对不会识别海。阈值在校准组固定，未反复调验证组。没有INT8、第三模型、训练或盲目降低门槛。

建议下一有界单元先收敛产品标签定义/独立困难负例，再物体检测负责人/鸟/车、场景分类负责山/海/森林/夜景，统一可安装包并串行CPU。原AI-W1§2限定只测两个候选；扩大路线需要崔总定案，已经在会话提供三个选项，不因无人回复擅自采纳。

## 已验证（Agent冒烟）

- `cargo test -p raybend --lib --features ai-probe`：**1414 passed / 0 failed / 9 ignored，21.05s**。包含真实jobs+catalog假编码器流程：提交、重复跳过、失败保留、推理间取消、屏蔽保留、删除/UID复用跳过；以及来源/撤销/桶/XMP/迁移边界。仍有同期develop测试unused_mut警告，不属于AI实现。
- `cargo test -p raybend-desktop --lib --features photo-ai-runtime`：**106 passed / 0 failed / 1 ignored，0.16s**，真实DTO与覆盖登记检查。首次漏5个DTO测试名称导致覆盖meta测试失败，已补并复跑通过。
- `cargo check --workspace` 与 `--features raybend-desktop/photo-ai-runtime` 均通过。AI特性默认未开，既有应用仍可编译。
- RAW worker原有独立进程集成：**7 passed，2.11s**；AI退出回归在阻塞管道中杀子进程并确认回收，不持进程锁读。
- `pnpm test`：**1200 passed / 0 failed**；typecheck已复跑通过；colors/i18n/arch/build最终均通过，build8.53s。保存的UI smoke最终在独立CDP端口9435运行，problems=[]；已有9333共享端口的一次运行出现根节点空，9434/9435隔离复测通过。记录竞争症状，不断言已证明具体竞态原因。冒烟日志过滤同时改为只忽略明确favicon URL的404，防止静态JS模块404被吞；变更后的脚本已复跑通过。新单元测试不依赖大模型/Python。校准脚本`python3 -m unittest discover -s scripts/ai -p 'test_*.py'`：2 passed，两报告由归档分数逐字节复现。
- Windows x64 debug ai-probe：官方CPU DLL绝对路径、CPU provider、参考误差6.348e-6、两张公开图两预处理worker误差0，退出/协议正常。Intel Ultra9 185H/16核22线程/Win11 build26200/约31.6GiB内存。首轮载入1.780s、P50/P95 66.7/118.2ms；最终载入0.724s、47.6/65.0ms，父/worker峰值分别218775552/219582464 bytes，1个子进程、退出后残留0。最终复测是热磁盘场景，不作为冷磁盘或完整应用性能承诺。仓内参数化PowerShell脚本已实际跑过（exit0/orphan0）。
- Rust/Python图片resize最大类别分数差0.002426，本次全部类别校准用实际Rust管线，不搬Python阈值。相同张量ORT/Torch误差分别6.348e-6/2.354e-6。

## 剩余与人类验收

生产模型未定案/未发布；HTTP安装、正式资源/DLL打包与默认运行特性尚未接线，因此当前软件不能启动七类正式识别。W1还缺产品定义一致的困难负例、预处理/真实RAW缺失率、SigLIP Windows与完整落库/冷磁盘/空闲卸载测量；模型不通过前不继续做无效量化或发布包。

Windows完整应用debug构建/check:win、GUI/来源弹窗/DPI/真实库/前台并发体感/XMP真实文件验收未做。smoke:ui只作为已有可重复脚本的DOM/日志冒烟；进程能起或合成分数能入库不能代替这些验收。

同步更新AI-W1/W2/W3、主题/分期规格、PLAN/FUTURE与Pencil同名md；不把下一路线写成已采纳，不将下期登记为FINISHED。
