# raybend · RayBend 核心库

RayBend（中文名「光伴」）的 Rust 核心库 —— 一个本地优先、开源的相片管理软件的媒体与编辑层。
桌面应用本体（Tauri 外壳 + Solid 前端）与全部设计文档在同一个仓库：
https://github.com/C-Thun/raybend

> ⚠️ **这不是可独立运行的应用程序，也不是稳定的第三方 API。**
> 0.x 期间接口随主线开发变动，不作 SemVer 兼容承诺；能力仍在从应用仓逐步搬入这个 crate，
> **目前不建议任何外部项目依赖它**。

## 包含什么

| 域 | 模块 |
| --- | --- |
| 库与索引 | `repo` / `index` / `store`（SQLite + 版本化迁移框架） |
| 导入 | `import`（落盘模版、序号、重名、RAW 分流） |
| 媒体与缩略图 | `media` / `thumbnail` / `fs_atomic` / `fs_asset` |
| RAW 解码 | `raw`（可插拔后端；解码在独立 worker 进程 `raybend-raw-worker` 里跑） |
| 编辑 | `develop`（非破坏性编辑栈）/ `color`（色彩管理） |
| 显示与渲染 | `display` / `render`（wgpu + WGSL） |
| 互通与周边 | `xmp`（sidecar 读写）/ `export` / `external_editor` / `lens` |
| 实验 | `ai`（本地图像语义索引，默认不启用） |

## 许可

**AGPL-3.0-only** —— 协议原文见仓库根目录 `LICENSE`；第三方组件与许可登记见
https://github.com/C-Thun/raybend/blob/master/THIRD-PARTY-NOTICES.md

## English

RayBend is a local-first, open-source photo management application (Windows-first).
This crate is its Rust core: repository index, RAW decoding, thumbnails, non-destructive
develop stack, and GPU rendering. It is **not** a standalone application, and it exposes
**no stable public API during 0.x** — the desktop app is built from the same repository.
