# 待人类协助事项（ASSISTANCE.md）

> **这是 Agent 与人之间的交接清单。**
>
> ## 铁律（同时写在 `AGENTS.md` 开头）
>
> 只要本文件存在**且有实际内容（不为空）**，则**任何任务在真正开始前（哪怕只是规划阶段）必须立即停止**，
> 并向用户提示先协助完成本文件中的事项。待用户确认完成后，Agent 需**逐项验证**，验证通过才可继续推进。
>
> ## 运作方式
>
> - **Agent 负责**：把需要人类做的事写进本文件的第一节；不要口头带过，也不要自己硬干。
> - **人类负责**：完成事项后告知 Agent。
> - **Agent 验证**：把结论与证据回填，并将该条移入「已完成」一节（保留记录，不删除）。
> - 本文件**长期存在**；「已完成」一节只增不删，作为间歇性协作的完整留痕。
>
> 最后更新：2026-09-15

---

## 一、待办（阻塞中）

### A1　美术设计阶段（M1 前置）

- **需要你做**：提供参考界面截图（Affinity / RapidRAW 等你想要的感觉），并**逐块口述**每个区域的方案。
- **Agent 随后做**：用 Pencil MCP 在 `design/` 下逐张绘制 `.pen` + 同名 `.md`。
- **为什么阻塞**：`PLAN.md` 的路线是 M0 → **UI 设计阶段** → M1。设计稿定案前不开工 M1 界面代码。
- **参考规格**：配色与风格规则见 `DESIGN.md`（基础色板已定）。
- **状态**：⏳ 等待你回来处理

### A2　Windows 上的 GPU / 渲染验证（M0-2 全部验证项、M0-6 帧率）

- **需要你做**：在 Windows 上配合运行并**目视确认**渲染结果。
- **背景**：WSLg **没有硬件 GPU**（实测 `MESA: error: ZINK: failed to choose pdev`、
  `libEGL warning: egl: failed to create dri2 screen`，是软件渲染回退）。
  因此 GPU 相关结论在 WSL 里一律无效，必须在 Windows 上取得。
- **待验证内容**（M0-2）：透明挖洞是否成立、DPI/缩放对齐、多显示器 DPR、窗口状态切换、
  坐标同步是否漂移、命中测试、后端回退与 device lost、4K 纹理性能基线。
- **另**：M0-6 的前端虚拟网格帧率也必须在 WebView2 上复测（WebKitGTK 数字无意义）。
- **状态**：✅ 前置已就绪（Windows 构建路径已打通，见 B1），等你回来开做 M0-2

### A3　RAW 测试样本（M0-3 / M0-5）

- **需要你做**：二选一或都给 ——
  1. 授权并从公开样本库下载（`raw.pixls.us` 等，部分需注册）；
  2. 提供你自己的照片目录路径（覆盖 CR3 / NEF / ARW / RAF / RW2 / ORF / DNG 中你能提供的格式）。
- **用途**：M0-3 格式支持矩阵与两条解码路径的耗时基线；M0-5 缩略图吞吐基准。
- **状态**：⏳ 等待

### A4　`DESIGN.md` 待决：中间色在 light 主题下的可读性

- **需要你定**：`DESIGN.md` §6 已知问题 1。
  实测 `#B6B0AF` 在浅色 `#F6F8F4` 上的对比度只有 **2.00 : 1**（WCAG AA 要求正文 ≥ 4.5:1），
  在深色 `#202226` 上有 **7.45 : 1**（完全可用）。
- **可选项**：① 派生一个「中间色深变体」专供浅底文字（实测 `#6E6866` ≈ 5.12:1 可过 AA）；
  ② `#B6B0AF` 在 light 主题下只用于非文字用途；③ 接受现状。
- **状态**：⏳ 待你决定

### A5　删除 Pencil 连接测试标记

- **背景**：为确认 Pencil MCP 能连上你打开的窗口，已在 `design/main.pen` 插入临时标记
  （节点 `ZZ_PENCIL_CONNECTION_TEST`，ID `D9ZRU`，黄色块 + 文字「Pencil MCP 连接测试」）。
- **你的答复（2026-09-15）**：「不保存就行了，无所谓」—— 无需 Agent 再调用删除接口。
- **已转入「已完成」B3。**

---

## 二、已完成（保留记录）

### B3　Pencil MCP 连接确认与测试标记处理

- **事项**：确认 Pencil MCP 能操作你打开的画布。
- **验证结果**（2026-09-15）：
  - `pencil_get_app_state` 读到 `design/main.pen`（顶层 frame `bi8Au`，800×600，layout:none，白底、空）；
    **`design/` 目录里实际不存在 `main.pen` 文件** —— 说明 `.pen` 由 Pencil 应用自身管理，不经文件系统暴露。
  - `pencil_execute` 插入标记 `ZZ_PENCIL_CONNECTION_TEST`（`D9ZRU`，子文本 `M82h1s`），
    截图确认已渲染到画布 —— **读写链路双向均通**。
- **标记的处理**：你选择都不保存（标记不会被写入磁盘），无需 Agent 调用删除接口。
- **状态**：✅ 已验证（人类目视截图确认）

### B1　Windows 侧 MSVC 与 Windows SDK 安装

- **事项**：安装 VS Build Tools（Desktop C++ 工作负载 + Windows 11 SDK）并升级 Windows 侧 Rust。
- **你于 2026-09-15 告知已完成**。
- **验证结果**（2026-09-15）：
  - VS C++ 工具集 ✅ `C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools`（VS 2026 版）
  - Windows SDK ✅ `10.0.26100.0`
  - Windows Rust ✅ `1.98.1` MSVC（且仓库的 `rust-toolchain.toml` 触发 rustup 自动安装了同名工具链）
  - **UNC 构建全链路实测通过**：`pushd` 映射 Z: → `cargo build --workspace` 3m14s 完成 →
    `raybend-desktop.exe`（7.8MB）落在 `C:\rb-target\raybend\debug\` → 进程启动存活（≈32MB 内存）→ 干净退出；
    WSL 仓库 `target/` 无 Windows 产物污染（中途踩了两个坑：cmd `set` 尾随空格、9p 不支持 rustc 锁文件，
    已用 `WSLENV` 透传 `CARGO_TARGET_DIR` 解决，命令已写入 `AGENTS.md` §5.3）
- **状态**：✅ 已验证

### B2　WSLg 窗口目视确认（M0-1 冒烟的人工部分）

- **事项**：运行 `pnpm tauri dev`，确认窗口标题、三栏布局、深色令牌、中文字体是否正常。
- **你于 2026-09-15 确认**：窗口已显示，一切正常。
- **状态**：✅ 已验证（人类目视）

---

## 验证记录

| 日期 | 事项 | 验证方式 | 结论 |
| --- | --- | --- | --- |
| 2026-09-15 | B2 WSLg 窗口 | 人类目视确认 | ✅ 正常 |
| 2026-09-15 | B1 Windows 工具链与 UNC 构建 | `vswhere` + SDK 目录 + `rustc -vV` + 实跑 `cargo build`/exe | ✅ 全部通过（细节见 B1） |
| 2026-09-15 | B3 Pencil MCP 连接 | `pencil_get_app_state` + `pencil_execute` 插入标记 + 截图 | ✅ 读写链路双向均通（细节见 B3） |
