# W4 —— 面板与画布：area panel / 层名条 / 控制点标记 / 眼睛

> 完成时间：2026-10-31 21:24:05
> 规格：`specs/editor-area.md` §5 ｜ 画稿：`design/editor.pen` `E6tDP` / `SPyhh` / `xjwv9` / `A0UG5v`

## 改动范围

| 文件 | 改了什么 |
| --- | --- |
| `src/api/editor.ts` | `Area` / `AreaCommon` 类型；`DevelopStack.areas/defaultIndex`；`sampleAreaSeed`（落点取样命令封装） |
| `src/api/types.ts` | `DevelopSettings.areas/defaultIndex`、`DevelopParamsPayload.areas/defaultIndex`、`EditorRenderState.overlayScale/overlayRotation` |
| `src/api/dto-contract.json` + `dto-contract.test.ts` | 上面两个新键 |
| `src/features/editor/store.ts` | 层状态（`areas` / `defaultIndex` / `activeAreaId` / `pendingPointAreaId` / 发号器）、`layersToRows`/`rowsToLayer`、参数 / 曲线按层路由、载荷与载入、重置清层、`showPoints` 偏好 |
| `src/features/editor/area-panel.tsx`（新） | 左列层列表：三种行（默认带锁 / 全图 / 控制点色点）、行高 30、整行拖动排序、双击行内改名、启停、删除（禁行图标）、待落点幽灵行、32 层禁用 |
| `src/features/editor/area-markers.tsx`（新） | 画布标记：圆片 20 / 白边 2.5 / 内芯 12、引线 70px、handle 9px、容差读数、`Ctrl` 拖动改采样位 |
| `src/lib/overlay-math.ts` + 测试 | 覆盖层指针 ↔ 图像像素（**只做 Rust 矩阵的逆变换**） |
| `src/features/editor/viewport.tsx` | 覆盖层入口（`overlay`）、落点态（单击 = 落点、`crosshair`） |
| `src/features/editor/toolbar.tsx` | left 第三个开关「编辑区」；right「显示控制点」眼睛 |
| `src/features/editor/left-panel.tsx` / `lut-panel.tsx` | 挂 AreaPanel；非默认层时 LUT 面板灰掉 + 浮条 |
| `src/features/editor/panels.tsx` | 层名条（22 高、`surface-track`、右缘渐隐）；非默认层：全局参数 / 降噪方式 / 镜头块 / 基础曲线 / 色彩管理禁用 + 浮条 |
| `src/components/ui/Histogram.tsx` | 176 → 164（图 138 + 控制行 26） |
| `src/lib/editor-chrome.ts` / `editor-prefs.ts` | 左列第三面板 `areas`；prefs **v4**（`showPoints`）+ 迁移 |
| `src/features/commands/catalog.ts` + `src/App.tsx` | 编辑区面板 / 新建控制点 / 新建全图 / 删除层 / 上移 / 下移 / 显示控制点 |
| `src/styles/tokens.css` | `--point-0..7`（控制点八色）、`--overlay-text-shadow` |
| `src-tauri/src/develop.rs` + `lib.rs` | `develop_area_seed` 命令（1024 解码 → 合成 → 特征向量） |
| `src-tauri/src/editor.rs` | `RenderState.overlay_scale/overlay_rotation`（标记尺寸钉屏幕像素用） |
| `src/workspaces/editor/EditorWorkspace.tsx` | 落点流程、`onAreaChange` → `confirmEdit`、标记挂进视口、confirm 节点清单补两行 |

## 关键决策与理由

1. **层是前端先建、整体提交**（与参数同样的覆盖式载荷）：`createArea` / `requestPointArea` 在内存里建层，
   松手（或结构动作结束）才 `confirmEdit` 落库。撤销由 W3 的 `diff_stack` 生成结构 op。
2. **控制点落点在 Rust 取样**（`develop_area_seed`）：种子必须来自"该层输入"，
   前端没有像素；命令解 1024 长边线性源 → 把当前栈（**不含待落点那一层**）合成一遍 → 取特征向量。
   镜头 / 降噪不参与（会移动像素）——已知近似，写在规格 §3.3。
3. **`Ctrl`+左键拖圆片改采样位**（崔总当天追加）：圆片与视口平移共用手势区，
   不加修饰键就会"想平移却拖走了点"；没按修饰键时**不拦事件**，照常交给视口平移。
   移动**不重取种子**（"换位置"与"重新取样"是两件事）。
4. **容差 handle 横向拖动**（往外 = 变大，400px 走完 0..1），末端显示 `容差 N` 读数；
   **不画范围圆**：容差是向量空间里的量，画圈等于宣称"圈外不算"。
5. **标记尺寸钉屏幕像素**：`overlayTransform` 负责位置，标记容器里用 Rust 报的
   `overlayScale` / `overlayRotation` 反向抵消（前端不自己推 zoom / DPR）。
6. **双击行内改名**而不是弹窗：层名是短文本，弹窗多一次确认与焦点跳转；
   `默认` 行不可改名、右侧画锁（它恒可用，画一个永远开着的开关是骗人）。
7. **非默认层的禁用**：右侧全局组（镜头 / 清晰度 / 局部色调 / 色彩管理 / 基础曲线）与 LUT 面板
   灰掉 + 一条 `当前编辑区域下不可用` 的浮条；`validate()` 里还有硬约束（Global 参数不许进层）。
8. **左列三选一**（编辑区 / 定稿 / LUT）：与已有两面板同一套 `editor-chrome` 状态机，
   再点同一个 = 关整列，`Tab` ③ 档的行为不变。
9. **眼睛是纯视图偏好**：`localStorage` v4 的 `showPoints`，不进 profile / 哈希 / 撤销 / XMP；
   关掉时标记不画、抓取一起关（不允许"看不见但能拖"）。

## 单测

前端（`src/features/editor/store.test.ts`）新增 6 条：层的新建 / 删除选邻居 / 上限 32、
参数按层路由（全局参数写不进层）、控制点落点与容差、颜色槽规则、
预设只认默认层、自动调整只写默认层。`src/lib/overlay-math.test.ts` 4 条（逆变换 / 旋转 / 退化 / 位移）。

## 验证

```bash
pnpm typecheck && pnpm test            # 1370 passed; 0 failed
pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n
pnpm build
```

## 遗留（需要崔总真机确认的部分）

* GUI 交互、多显示器 / DPI、真实照片下的标记手感、拖动手势（看不见的都要人看）。
* 设计稿的三张派生状态帧（裁切 / 旋转 / 对比）左列还是旧版；toolsbar 挤压态几何未重量。
* 「该层输入已变 → 重新取样」按钮（规格 §5.6）本轮未做：取样命令与层数据都已就位，缺一个入口。
