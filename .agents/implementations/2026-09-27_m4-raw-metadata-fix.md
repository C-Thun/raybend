# M4 验收回报：RW2 导出元数据兼容修复
完成时间：2026-09-27 01:30:36 CST

## 范围与原因

崔总截图中 MYP1000036 的两张定稿失败，错误为 `不支持: Invalid forty two`，同队列 SOOC 项成功。只读核对失败源为 Panasonic RW2，文件头 `49 49 55 00 18 00 00 00`。

导出 `Metadata::read` 未复用 media::tiff 的 RAW 头兼容规则：读取拍摄 EXIF 失败后被忽略；native_xmp 又把任何 II/MM 前缀当作标准 TIFF，用只接受魔数 42 的 kamadak-exif 解析，最终阻断输出。不是 AVIF 编码器故障；共享元数据链使普通导出和外部 TIFF 都受影响。

修复首个错误后，只读样本探针又定位到该 RW2 的 XMP（tag 700、Undefined）计数包含终止 NUL，严格 XML 解析继续报错。两处均已补回归。

## 实现与文件

- `crates/raybend/src/media/tiff.rs`：提取已有 parse/read_fields 共用的 TIFF/RW2/ORF 头识别；增加 typed EXIF 适配，只把解析器持有的内存副本魔数标准化为 42，IFD 偏移与类型保持原样，绝不改写源文件。
- `crates/raybend/src/export/metadata.rs`：EXIF 后备读取及 TIFF 家族 native XMP 统一使用上述入口。只靠 II 前缀的非 TIFF CRW 不再误判。XMP XML 解析前只去除末尾 NUL 填充，内部 NUL、非法 XML、尾部垃圾仍报错。
- 合成 RW2/ORF 各大小端变体验证拍摄信息、有理数精度、GPS、中文 XMP、Undefined 与终止 NUL、源字节不变；损坏 IFD 与非 TIFF 前缀边界保留。
- 新增显式 ignored 的 `source_metadata_smoke`：需要 `RAYBEND_EXPORT_METADATA_SOURCE` 指定只读样本；读取真实元数据，配 8×6 合成 RGB16 像素验证五格式编码和读回。不会把真实 RAW 解码或写出照片，也不自动跑用户库。
- 当前任务记录更新于 `specs/M4-finalization.md`。没有界面、交互或新增命令，Pencil 与命令/热键无需变化；未触碰其他 Agent 的选择语义实现。

## 已验证（冒烟）

- 修前新增合成回归确实失败：`Unsupported("Invalid forty two")`，日志 `/tmp/m4-raw-metadata-before.log`。
- `cargo test -p raybend --lib`：1167 通过、0 失败、6 ignored，24.98 秒。
- Windows `cargo test -p raybend --lib export::metadata::tests`：5 通过、0 失败、1 ignored。
- WSL / Windows 分别显式运行只读样本探针：MYP1000036.RW2 的 20 项拍摄元数据成功写入 AVIF/WebP/JPEG/PNG/TIFF，并读回相机制造商字段；原源字节完全一致。两侧均通过。
- `git diff --check` 通过。只格式化本次 Rust 文件，未重排整仓。
- `pnpm debug:win` 完成，包含前端 build 和 `check:win`。主程序 2026-09-27 01:29:57 CST，dist 01:28:22，4 个引用资源全部命中；RAW worker 01:29:07，包含当前 raybend-worker-proto-v3 且比相关源码新。构建仅有既有的前端 chunk 大小提示。

日志 `/tmp/m4-raw-metadata-{tests,probe,win-tests,build}.log`。未发布、push、tag 或生成安装包。

## 验证边界

M4 开发和 Agent 冒烟已完成，崔总正在真机验收，本次是验收发现的缺陷修复。上述样本检查只验证元数据与合成像素编码，不能声称真实照片出图、GUI 操作、颜色或性能 E2E 已通过。重开新版后需把刚才失败的定稿重新入队确认；队列按既定规则仅在内存中，不跨应用重启保存。
