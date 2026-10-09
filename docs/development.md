# RayBend 开发命令索引

> 开发期命令的完整索引；发布、签名与更新通道见 [release.md](release.md)。
> 最快上手：`pnpm install` → `pnpm tauri dev`。

## 日常开发

| 命令 | 作用 |
| --- | --- |
| `pnpm dev` | 只起 Vite 前端 —— 下面那些浏览器检查要连的页面 |
| `pnpm build` | 前端构建到 `dist/`（**跑任何 Windows 构建之前先跑它** —— `dist/` 是编译期嵌进 exe 的） |
| `pnpm tauri dev` | 起桌面窗口 |
| `pnpm tauri <参数>` | Tauri CLI 透传；AI 资源按 `--ai=auto\|required\|off` 解析 |
| `pnpm preview` | 预览构建后的前端 |

## Windows 构建与调试

产品版在 Windows 侧验证；跑之前必须先 `pnpm build`（见上）。

| 命令 | 作用 |
| --- | --- |
| `pnpm debug:win` | 一键 debug 构建：前端 → Windows cargo → 产物核对（可接 `-- --ai=…`） |
| `pnpm check:win` | 产物体检：与 `dist/` 的时间戳、内嵌前端资源、worker 协议 |
| `pnpm clean:win` / `pnpm clean:wsl` | 只清理失效 target 产物，不付冷构建的代价 |
| `pnpm check:color-win [profile…]` | Windows 侧色彩探针（在 Windows 目标上构建并跑色彩示例） |
| `pnpm check:lens-ipc-win --launch <库 id> <资产 id>` | 只读的镜头元数据 IPC 冒烟；`--expect-profile=` / `--expect-metadata-warning` 加断言 |

## AI 资源（离线相片标签）

识别全在本机：模型包与 CPU 运行库都是**构建输入**，不在运行时下载。
构建命令（`pnpm tauri` / `pnpm debug:win` / `pnpm release`）都接 `--ai=auto|required|off` —— `auto`（默认）用已登记的包，没有就回退到无 AI 版（会打印原因）；`required` 要求每个输入都在且已校验，否则失败；`off` 不碰模型源也不联网。

| 命令 | 作用 |
| --- | --- |
| `pnpm ai:prepare [--ai=auto\|required\|off]` | 解析并冻结本次构建用的 AI 输入（模型包 + CPU 运行库 DLL），把计划打成 JSON |
| `pnpm ai:use [-- <模型库>]` | 登记**已经导出好**的包（新机器 `git clone` + `git lfs pull` 后就够） |
| `pnpm ai:library:init [-- <路径>]` | 在指定路径初始化模型库；不传路径就用本机默认位置 |
| `pnpm ai:export [-- <模型库>]` | 经模型库导出/校验包，并把源登记给本项目 |

模型本体在**独立的 `model-registry` 仓库**里，与 raybend 检出并列放。raybend 的 Git 只保留兼容契约与准入摘要；本机源指针（`ai-model-source.local.json`）被 Git 忽略。

```bash
# 一次性：把模型库放到 raybend 旁边
git clone https://github.com/C-Thun/model-registry ../model-registry
cd ../model-registry && git lfs install && git lfs pull   # ONNX 编码器走 LFS：指针不是权重

# 登记包并构建
cd ../raybend
pnpm ai:use -- ../model-registry     # 包已导出且已提交：只需登记
pnpm debug:win -- --ai=required      # 想要无 AI 版就 --ai=off
```

从 0 开始构建 ONNX 包（只在模型或配方变了时才需要；要求 Python 3.12、能联网、≥4 GiB 空闲空间与 Git LFS）：

```bash
cd ../model-registry
pnpm model:export                    # 固定上游 revision → FP32 / opset17 ONNX → 库内提交

cd ../raybend
pnpm ai:library:init -- ../model-registry   # 库还没有归属标记时才需要
pnpm ai:export -- ../model-registry         # 导出 + RayBend profile + 本机源登记
pnpm ai:prepare -- --ai=required            # 构建前校验并冻结输入
```

注意：只用 CPU，产品不依赖 CUDA / Python / PyTorch；`RAYBEND_AI_RUNTIME_DIR` 可指向本机 onnxruntime 目录，替代已核对缓存与固定官方 ZIP；重新导出导致摘要变化属**待审阅事件**，不会自动更新（协议 [specs/ai-model-library-build.md](../specs/ai-model-library-build.md)，v1 阈值与质量 [docs/ai/tinyclip-v1/README.md](ai/tinyclip-v1/README.md)）。

## 质量门

| 命令 | 作用 |
| --- | --- |
| `pnpm typecheck` | `tsc --noEmit` |
| `pnpm test` | 前端单元测试 |
| `pnpm test:release` | 发布脚本自己的单元测试 |
| `pnpm lint:colors` | 颜色令牌层之外不许出现硬编码颜色 |
| `pnpm lint:arch` | 前端分层与依赖方向 |
| `pnpm lint:i18n` | 翻译覆盖 |
| `cargo test --workspace` | Rust 测试（图像、RAW、存储、迁移） |

## 冒烟与回归

浏览器类检查需要另开一个终端跑 `pnpm dev`。

| 命令 | 作用 |
| --- | --- |
| `pnpm smoke:ui [url]` | 页面真的渲染出东西 —— 防白屏的控制台报错闸门 |
| `pnpm check:startup` | 启动 / 系统偏好 |
| `pnpm check:flowbar` | 工作流切换 |
| `pnpm check:browse` | 库非空时进浏览 |
| `pnpm check:import` | 导入确认流程（走到 `import_start`） |
| `pnpm check:export` | 导出工作区启动 |
| `pnpm check:external` | 外部编辑器往返 |
| `pnpm check:color-status [url]` | 色彩管线状态探针（显示配置与变换状态） |

## 性能

| 命令 | 作用 |
| --- | --- |
| `pnpm perf:browse [行数]` | 网格虚拟滚动基准（默认 10 万行） |
| `pnpm perf:grid [行数]` | 网格管线吞吐：按时间分组、行模型、虚拟窗口 |
| `pnpm perf:win [--launch]` | Windows 真机浏览采样（CDP） |

## 周边与历史脚本

这里的东西都不在构建或发布链上。

| 命令 | 作用 |
| --- | --- |
| `pnpm shot` | 按主题 / 密度 / URL / 尺寸截图 —— 开发期视觉自查 |
| `pnpm crash:drill` / `pnpm migrate:drill` | M1 存储期的演练（worker 崩溃隔离；catalog 迁移 / 备份 / 故意损坏）—— 动恢复逻辑时重跑 |
| `pnpm spike:win` | M0-2 / M2-W1 的渲染 spike（构建 → 开窗 → 测量 → 落报告），留作测量工具 |
| `scripts/build-dav1d-win.cmd` | Windows 侧一次性构建 dav1d 静态库的辅助脚本（其环境变量封在 `scripts/lib/dav1d-win.mjs`） |
| `scripts/ai/*.py`、`scripts/ai/*.ps1` | 模型工作的研究 / 归档脚本（阈值复算、参考向量、预处理一致性、Windows worker 探针）—— 不是产品依赖 |

内部模块在 `scripts/lib/`（`release-*.mjs`、`ai-*.mjs`、`cdp.mjs` 等）：它们是上面这些入口的库，本身不是命令。
