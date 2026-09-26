# M5 直下发行链收口

开工：2026-09-27；承接 HANDOFF-2026-09-27-m5，崔总授权连续完成独立工作。保留当前工作树和未跟踪文件；不生成安装器、签名、tag、push 或上传。

本工作单元复用 release / finalize / 官网构建期下载数据。当前没有 plannotator/todo 接口，未虚构评审；按已有授权推进。

- [x] 核对 M4 导出/外部编辑与 M5 更新保护，补双语离线帮助。
- [x] finalize 产出完整待上传目录，拒绝旧文件混入；发布指令默认预览，崔总显式 --execute 后完成精确提交/tag/push/可重试草稿上传/公开并等待官网，配合成单测。
- [x] 官网复用 Release 数据适配，过滤草稿/预发布/不合规资产，API 失败不虚构版本；配构建适配单测。
- [x] 通过 Pencil MCP 核对 M5 节点并尝试独立验证持久化，受工具限制时明确记录。
- [x] MSIX 记录 FUTURE，更新路线/操作说明，完成质量门和实施记录。

真实安装、更新、卸载、Windows 信任、照片/GUI/色彩/外部软件 E2E 留给崔总。MSIX 不属本轮 DoD；本轮聚焦 NSIS/MSI → GitHub Release → 官网下载。

追加授权：崔总要求最终交付每版不超过三条指令；现收敛为本地准备与显式发布两条，工具/账号/Pages 一次性配置另列，实际执行仍由崔总完成。Pencil 节点核查通过，独立副本未被 MCP 打开（落回当前画布）；computer-use 因 WSL sandboxCwd URI 无法初始化，磁盘独立重载未确认。

收口证据：`implementations/2026-09-27_m5-two-command-release.md`。45 项发布模拟回归、1059 项应用单测、95 项桌面 Rust/1 ignored、24 项官网单测与相关质量门通过。实际 Windows bundle/签名/安装/更新/GitHub/Pages 未执行；M5 发行 DoD 未完成。
