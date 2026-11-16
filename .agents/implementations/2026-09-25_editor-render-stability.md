# 编辑器换图、显影与 GPU 视口稳定性修复
完成时间：2026-09-25 13:31:41 CST

## 范围与证据

- 人工复现后的 `/mnt/c/src/tmp/rb-editor.log` 为 0 字节，未留下原先预期的 `[editor]` 标记。Windows Application 事件日志记录到两次 `raybend-desktop.exe` 崩溃（11:46、11:53，异常 `0xc0000409`、子码 7，故障偏移 `0x19cd031`）。现存新版 exe/PDB 对该偏移的符号解析落在 `std::alloc::rust_oom`；发生崩溃时的 exe 比现存文件早一版，因此这强烈提示内存耗尽，但不能当作完整崩溃调用栈。
- 在 `src/workspaces/editor/EditorWorkspace.tsx` 找到确定的前端重复工作：换图 effect 同步读取 `developRev()`，使每次拖动参数都重新执行换图清理、读编辑栈与预览刷新。缩略图对象引用变化也会触发不必要的重载。

## 改动与理由

- `src/workspaces/editor/EditorWorkspace.tsx`：换图只依赖稳定的 asset id；用 `untrack` 读取 revision 快照；换图立即清除旧参数和旧显示状态；异步读编辑栈、镜头匹配、解析照片路径均检查当前选择；进入照片不再并发刷新 latest 预览，离开时仍在落库后刷新；切换后重发目标照片的参数；清理时把完整镜头/降噪/色温字段落库。
- `src/features/editor/source.ts`、`index.ts`、`model.test.ts`：只有当前目标的 `paintedPath` 才开放 GPU 透明洞口，旧帧不能掩盖正在载入状态；补边界单测。
- `src-tauri/src/editor.rs`：照片请求与参数请求设序号闸门，丢弃晚到的旧请求；高频镜头库查询/解析移入后台阻塞任务并按照片和镜头选择缓存；渲染循环每批最多消费 32 条命令后尝试绘制，并合并同批尺寸变化；修复首条 Stop 被忽略；换图先释放旧 GPU 纹理和旧线性 RAW 源以压低内存峰值；补闸门/resize 单测。原会话添加的过渡帧、真帧诊断及手动 RAW 复现测试保留。
- `scripts/check-win-artifact.mjs`：worker 新旧判定只比较核心库源码，桌面壳源码单独比较 exe，避免只改 `src-tauri` 后误报 worker 过期；同时检查相关 Cargo 清单。
- 新增功能：无新增用户动作，命令目录/默认热键不适用。

## 冒烟验证

- `pnpm test`：当时 906/906 通过；之后新增的视口状态单测 `node --test src/features/editor/model.test.ts`：22/22 通过。
- `cargo test -p raybend-desktop --lib editor::tests`：22 通过、1 条真 RAW 手动样本测试忽略。
- `pnpm typecheck`、`pnpm lint:arch`、`pnpm lint:i18n`、`pnpm lint:colors`、`node --check scripts/check-win-artifact.mjs`、`git diff --check`：通过。
- `pnpm debug:win`：前端生产构建、Windows Rust debug 构建、`check:win` 和 target 清理均完成；最终 exe `/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`，时间 2026-09-25 13:29:09 CST，内嵌前端资源均匹配，worker 含当前 `raybend-worker-proto-v2`。
- `cargo fmt --all -- --check` 遇到仓库既有的大量格式漂移，未对无关文件批量格式化。

## 遗留与人工验证

- AGENTS.md 规定 GUI 交互、窗口拖动、视觉与真实照片 E2E 由人类在 Windows 环境执行；此次只完成冒烟，不声称崩溃、卡顿或挖孔拖动已通过真机复测。
- GPU 初始化仍发生在进入编辑器时。提前预热需要把 GPU 设备生命周期与编辑窗口 surface 生命周期拆开；当前修复先消除确定的重复任务、排队饥饿与换图内存叠加。是否需要进一步架构调整，以人工复测和可靠 stderr/崩溃转储结果为依据。
- 若仍崩溃，应在 Windows 原生进程中捕获 stderr 或转储；WSL 背景启动重定向未写出日志。
