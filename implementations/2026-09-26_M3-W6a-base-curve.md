# M3-W6a 可选基础曲线与机型档案
完成时间：2026-09-26 03:49:14 CST

## 范围与文件

- Rust 显影：`crates/raybend/src/develop/curve.rs`、`pipeline.rs` 在显示编码和用户曲线之间加入可选基础曲线；`thumbnail/render.rs` 对 RAW latest 读取同一份曲线快照，并提升缓存管线版本。
- 数据：`store/base_curve.rs`、app schema v5、catalog schema v10。app.db 仅按 EXIF 品牌+型号列出多档案；编辑栈保存选择 ID 与快照。旧编辑栈显式迁到 `none`。
- 桌面和前端：`src-tauri/src/base_curve.rs`、`develop.rs`、`editor.rs`；`src/api/editor.ts`、`types.ts`，普通载图与撤销共用同一编辑栈设置适配，编辑 store、曲线画布、面板和双语文案。新照片不自动选基础曲线；SOOC 侧不显示也不渲染基础曲线。
- 计划与设计：`PLAN.md`、`FUTURE.md` 的失效交叉引用、`plans/M3-W6a.md`、`design/editor.md`；Pencil 的 `design/editor.pen` 已包含曲线下方选择器与展开态，当前文件与 Git 索引相同。

## 决策

- 不按 ISO 分档，不提供品牌或通用保底曲线；没有完整品牌和型号时隐藏基础曲线功能。
- 选中档案时保存曲线快照，使以后档案统计变化不改变旧照片的渲染。基础曲线复用现有 `Curve` 求值器，用户曲线四通道控制点不变。
- 基础曲线选择是局部控件，不适合进命令面板；未新增热键。

## 验证与遗留

- `cargo test -q -p raybend --lib`：1073 通过、2 忽略；涵盖旧栈迁移、机型隔离、RAW/SOOC 门控及曲线顺序。
- `cargo test -q -p raybend-desktop --lib`：84 通过、1 忽略。`pnpm test`：950 通过。`pnpm typecheck`、`pnpm lint:arch`、`pnpm lint:i18n`、`pnpm lint:colors`、`pnpm build` 通过。
- `pnpm debug:win` 构建成功，`check:win` 核对 exe、dist 和 RAW worker 成功。真实 Windows GUI、相机样张色彩与交互未经人类 E2E 验证。
- 这轮没有进入 W6c 的 LUT/issue 工作。
