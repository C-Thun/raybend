# W6 —— 收尾：i18n / 快捷键 / 文档同步 / 质量门

> 完成时间：2026-10-31 22:31:18
> 规格：`specs/editor-area.md` ｜ 交接书 `specs/editor-area-handoff.md` §8 / §9

## 改动范围

| 面 | 做了什么 |
| --- | --- |
| i18n | 新增 26 个 key（zh-CN + en-US 成对）：`editor.area.*`（面板 / 三种层名 / 新建 / 删除 / 改名 / 上限 / 浮条 / 落点提示 / 容差读数 / 眼睛 / `Ctrl` 拖动提示 / 上移下移）、`editor.toolsbar.areas`、`cmd.editor.area*`、`cmd.editor.showPoints` |
| 快捷键 | 命令面板新增 7 条：编辑区面板、新建控制点、新建全图、删除当前层、上移、下移、显示控制点。**全部不设默认热键**，每条在 `catalog.ts` 上写了留空理由（需要画布落点 / 结构动作误触代价高 / 方向键无约定） |
| 文档 | `specs/editor-area.md`（状态牌 + 波次表 + 六节实现注 + 预设 / 命名 / 标记口径）；`specs/editor-area-handoff.md`（状态牌 ✅ + 三条实现交接补充）；`design/editor.md`（2026-10-31 节补 4 条实现补充）；`memory/LIMITS.md`（area 32 的代码真相 + 层名 30 字符） |
| `.png/.pen` | **未改**：画稿四帧是 W0 的定稿，实现按它走；实现里偏离的三处（行内改名 / 斜向引线 / 横向容差拖动）写进 `design/editor.md` 的补充条目，不再回改画稿 |
| 实施记录 | W2 / W3 / W4 / W5 各一份 + 本文件 |

## 质量门（命令原文与读数）

```bash
$ cargo check --workspace
    Finished `dev` profile [optimized + debuginfo] target(s) in 7.91s
    （3 条 warning：`Priority::Foreground` 未使用等既有告警，非本次引入）

$ cargo test --workspace
    raybend (lib)            1621 passed; 0 failed; 11 ignored
    raybend-crashpad           7 passed; 0 failed
    raybend (raw_worker)       4 passed; 0 failed
    raybend-desktop (lib)    192 passed; 0 failed;  2 ignored
    raybend-desktop (main)     2 passed; 0 failed
    raybend-writer             2 passed; 0 failed
    → 合计 1829 passed / 0 failed

$ pnpm typecheck
    tsc --noEmit → 无输出（通过）

$ pnpm test
    ℹ tests 1370  ℹ pass 1370  ℹ fail 0

$ pnpm lint:colors
    ✓ 无硬编码色值（色值仅出现在 tokens.css）

$ pnpm lint:arch
    ✓ 分层依赖合规（memory/ARCHITECTURE.md §1/§2）

$ pnpm lint:i18n
    ✓ 无硬编码中文文案（界面文字都走语言包）

$ pnpm build
    ✓ built in 4.29s（`dist/` 已更新）

$ pnpm debug:win
    ✓ built in 9.98s（前端）
    Finished `dev` profile [optimized + debuginfo] target(s) in 4m 29s（Windows 侧 cargo）
    ✓ Windows 产物与前端一致
    exe:  /mnt/c/rb-target/raybend/debug/raybend-desktop.exe（2026-10-31T14:35:14.928Z）
    dist: 4 个引用资源全部命中
    worker: /mnt/c/rb-target/raybend/debug/raybend-raw-worker.exe（raybend-worker-proto-v7）
```

测试产物：`/mnt/c/rb-target/raybend/debug/raybend-desktop.exe`（`pnpm check:win` 已核对时间戳与资源）。

## 与交接书的差异（都已在规格里改口径）

| 交接书说 | 实际 | 原因 |
| --- | --- | --- |
| 改名用 `Dialog` | **行内输入框**（双击行） | 层名是短文本，弹窗多一次确认与焦点跳转；`Esc` / `Enter` 语义与弹窗一致 |
| 拖动圆片改位置 | **`Ctrl`（`⌘`）+ 拖动** | 崔总当天追加：普通拖动留给视口平移，否则"想平移却拖走了点" |
| 容差 handle 拖动 | **横向**（往外变大） | 与斜向引线末端配套；读数显示 `容差 N` |
| 引线"向可用空间那一侧水平引出" | **斜上 45°、70px 固定**，左右按图像中线翻边 | 画稿 `xjwv9` 的两态；量标签宽度做动态翻边会在缩放时来回抖 |
| 预设"作用在当前选中的 area" | **恒以默认层为作用对象** | 一半大类只能挂在默认层，按层套用会静默丢项（见 W5 记录决策 1） |
| 新层名"全图 1 / 控制点 1" | **留空 → 类型默认名**，可改名 | 与 W1 的 `Area.name` 语义一致（空 = 默认名），序号在短标签上没有信息量 |

## 交付给崔总测试的清单（GUI / 真机项，未经自动化验证）

1. 新建三个控制点各自调色 → 看是否只有预期区域变；
2. 调容差 → 看渐变宽度与"纳入多少"是否跟着变；
3. `Ctrl` 拖动圆片 → 只换采样位（画面会因为种子不变而"不动"，这是预期）；
   想换种子要重新落点；
4. 与全图层排序 → 看 LUT 在默认层前后（默认层在第 1 位 = LUT 先作用）；
5. 撤销十步（含建层 / 删层 / 排序 / 改名 / 启停 / 容差）；
6. 存定稿 → 切 SOOC → 切回来（层必须一起回来）；
7. 存预设 → 套到另一张（层不动、打印值来自默认层）；
8. 导出 → 与编辑器画面一致。

## 已知未做（不在本期范围）

* 封闭控制点（`memory/FUTURE.md` D14）、蒙版 / 画笔、层不透明度 UI、层复制粘贴、三路色彩圆盘。
* 「该层输入已变 → 重新取样」入口（数据与命令都在，缺按钮）。
* 工作域（v2 色彩路径）下层链的 >1 高光精度（见 W2 记录决策 7）。
