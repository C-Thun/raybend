# README 门面化精简

完成时间：2026-10-09 22:43:37

## 改动范围

应崔总要求（「README 不是决策与缺口登记册，有什么介绍什么」）对双语 README 做门面化重写，
并把开发期命令索引整体迁出 README。

- `README.md`、`README.zh-CN.md`：209 行 → 71 行，两侧结构逐节对齐。
- 新增 `docs/development.md`：承接原 README「命令行」全部内容（日常开发 / Windows 构建调试 /
  AI 资源 / 质量门 / 冒烟回归 / 性能 / 周边历史脚本），发布侧仍以 `docs/release.md` 为准。

## 删除内容与去向

| 删除项 | 理由 | 去向 |
| --- | --- | --- |
| 「两条刻意独立的路径」（内嵌 JPEG 小图 vs worker 完整解码）整段架构说明 | 内部实现决策，用户不需要知道解码走哪条路 | 仍在 `memory/ARCHITECTURE.md` / `memory/FUNCTION-IMAGING.md` |
| 「已知缺口登记」段（Foveon 不做、尼康 HE/HE★、`arw6`、哈苏 `.FFF`、`.ORI`） | 对外不列自己的问题清单；一句「覆盖随 rawler 上游推进」已如实告知 | `memory/FUTURE.md` §B 原样保留 |
| 命令行 7 张表（含 release 命令表、AI 从零构建、crash/migrate 演练、spike 等） | README 不是内部脚本索引 | `docs/development.md`；release 表不迁移，`docs/release.md` 已有 |
| 「为什么做光伴」独立小节 | 与顶部口号重复，压成一段开场白 | —— |

## 保留内容

门面信息：品牌横标与语言/官网/下载链接、功能九条、格式表（照片/导出/RAW 扩展名）、下载、
从源码构建三条命令、技术栈、文档索引、许可。HEIC 保留一行脚注（会被导入建索引、预览待解码器），
避免列入格式表却不作说明。

## 验证方式

- 两 README 与 `docs/development.md` 全部相对链接用脚本逐条检查，无 MISSING。
- 行数核对：README 双语 71 / 71，`docs/development.md` 114。
- markdown 检查工具运行无 error（仅有既有 inline HTML 类 advisory）。

## 遗留问题

无。若崔总认为 HEIC 脚注或「技术栈」等仍属多余，可再删一行级别的内容。
