# 编辑区（area）系统 —— 换手交接书（W2–W6 一次做完）

> **给接手的新会话**：这份文件是**唯一入口**。读完它就能接着做，不需要重新推导设计。
> 设计事实在 `specs/editor-area.md`（口径）与 `design/editor.pen`（画稿），本文件只负责
> **「现在到哪了 / 接下来怎么做 / 别踩哪些坑 / 什么算做完」**。
>
> 崔总 2026-10-31 的要求：**换手后一次把剩下的波次干完，做完交给他测试。**
> 所以：登记 todo → W2 → W3 → W4 → W5 → W6 **连续推进**，只在「真需要人类」或「继续会踩空」时停。

## 0. 状态牌

| 波 | 交付 | 状态 |
| --- | --- | --- |
| W0 | 设计稿（`design/editor.pen` 四帧） | ✅ 2026-10-31 崔总「没问题」 |
| W1 | 类型与迁移（数据形状 / DB / 定稿 v3 / 参数 `target` / 控制点八色） | ✅ 测试全绿 |
| **W2** | **渲染：链循环 + 层内固定管线 + 控制点权重 + `PIPELINE_VERSION`** | ✅ 2026-10-31 |
| **W3** | **撤销与持久化：`Op` 加 `area_id` + 结构 op** | ✅ 2026-10-31 |
| **W4** | **面板与画布：DTO / area panel / 层名条 / 控制点标记 / handle / 眼睛 / 改名** | ✅ 2026-10-31 |
| **W5** | **周边：预设过滤 / XMP 归类 / auto_adjust 限定默认层** | ✅ 2026-10-31 |
| **W6** | **收尾：i18n / 快捷键 / 文档 / 实施记录** | ✅ 2026-10-31 |

> **下一步（修 bug 阶段）看 `specs/editor-area-bugfix-handoff.md`** —— 那里是「现在到哪了 / 已知问题清单 / 验收清单」，
> 本文件只作为 W2–W6 的历史交接记录保留。
>
> 实施交接补充（2026-10-31）：`Ctrl`+左键拖圆片 = 移动采样位（崔总当天追加）；
> 容差 handle 改为横向拖动；层改名走**行内输入**而不是弹窗；预设**恒以默认层为作用对象**。
> 三条都已写回 `specs/editor-area.md`（§3.3 / §5.1 / §5.4 / §5.9）。

**W2 与 W3 文件不重叠，可以并行**（一个碰 `develop/`，一个碰 `store/marking.rs`）；W4 依赖 W1（DTO 那一段）。

## 1. 现在代码里已经有什么（事实，不用再查）

### 1.1 数据形状（`crates/raybend/src/store/develop.rs`）

```rust
pub struct DevelopStack {
    // 默认层本体（与本次改造前逐字段一致）
    color, source_base, params, curves, as_shot_k,
    lens_profile, lens_enabled, base_curve_profile, base_curve_points,
    lut_id, lut_enabled, nr_method, geometry, auto_adjust,
    // 默认层之外的层
    areas: Vec<Area>,            // 空 = 只有默认层（旧数据全是这种）
    default_index: Option<u8>,   // 默认层在层列表里的位置（0-based）
}

pub struct Area {
    id: u32, kind: AreaKind, name: String, enabled: bool,
    opacity: f32, color: u8,             // color = 控制点色槽位 0..8
    params: BTreeMap<String, f64>, curves: BTreeMap<String, Vec<[f32;2]>>,
}

pub enum AreaKind {
    Full,
    Point { x: f32, y: f32, seed: [f32;3], tolerance: f32 },
}
```

**三件已经钉死的事**：

1. **默认层不是 `areas[0]`**，就是 `DevelopStack` 自己（`params`/`curves` + 全局那几项）。
   层列表 = `[默认] + areas`，默认层的下标是 `default_index`（`None` 时视为 0）。
2. **退化形状零成本**：`areas` 为空 / `default_index` 为 `None` 时**整个字段不序列化**
   ⇒ 旧字节、旧 `profile_hash`、旧 `signature()` 逐字不变。**任何新字段都要沿用这个规矩。**
3. `AreaKind::Point` 的 `tolerance` 是**向量容差**（无量纲 0..=1），**不是空间半径**。
   控制点是**全图向量匹配**，没有空间项（§4.2 的硬口径）。

常量：`MAX_AREAS = 32`（不含默认层）、`POINT_COLORS = 8`、`MAX_AREA_NAME_CHARS = 30`。

已实现的方法：`validate()`（层上限 / ID 唯一 / 名字长度 / 不透明度 / 颜色槽位 / 坐标与容差 / 全局参数不许进层）、
`next_point_color()`（取已用最少，并列取下标最小）、`signature()` 在有层时追加一段 `|areas:…`。

### 1.2 参数表（`crates/raybend/src/develop/params.rs` + `assets/develop-params.json`）

`ParamSpec.target: ParamTarget { Global | Layered | Mask }`，17 条已分类：

* **Global**（只能挂默认层）：`dynamicContrast`、`lumaNr`、`colorNr`、`sharpenAmount`、`sharpenRadius`、
  `distortion`、`vignette`、`vignetteRange`、`chromatic`、`chromaticBlue`；
* **Layered**（三种层都行）：`exposure`、`contrast`、`highlights`、`blacks`、`temperature`、`saturation`、`vibrance`；
* **Mask**：暂时一条都没有 —— 四条 `area*`（`areaTolerance` / `areaOpacity` / `areaChroma` / `areaLuma`）
  **故意推迟到 W2 再加**，避免 W1 往面板里塞四条无消费者的滑杆。

契约**三处必须同步**：Rust `PARAMS`、`assets/develop-params.json`、前端 `src/features/editor/params.ts`（读同一份 JSON）。
`params_match_the_frontend_contract` 会逐条比对（含 `target`），漏一处就红。

### 1.3 DB 与定稿

* 迁移：`store/migrations/catalog_0023_areas.sql`（`ALTER TABLE develop_stacks ADD COLUMN areas TEXT`，**不回填**）；
  值形状是 `{"defaultIndex":n,"areas":[…]}`，只在有额外层时写。
* `prune_empty_stack` 已加 `AND areas IS NULL`。
* **两个版本号是两件事，别再踩**：
  * `profile_schema_version(&stack)` —— **按内容选**：有层 → 3，有 color → 2，否则 1；
  * `PROFILE_SCHEMA_VERSION` —— **读取上限**，已提到 **3**。
    （W1 时只改了前者忘了后者，结果是"写完读不回"；已修。）

### 1.4 控制点八色（`memory/DESIGN.md` §15.5）

固定八色、**存槽位下标不存色值**：`#EF884C #4CE4EF #EAEF4C #4C62EF #7DEF4C #BE4CEF #4CEF88 #EF4C98`。
`design/editor.pen` 里有同名变量 `point-0..point-7`。与 browse 色标（`--label-*`）刻意分家。

### 1.5 设计稿（W0 已过）

| 帧 | id | 画了什么 |
| --- | --- | --- |
| `Components / Editor / 编辑区（area）` | `E6tDP` | 面板三种选中态 + 行原子 9 态 + 边界 3 态 |
| `Components / Editor / 层名条` | `SPyhh` | 直方图 176→164 + 22 高的层名条三态 |
| `Components / Editor / 控制点标记` | `xjwv9` | 圆片 / 引线 / 名字 / handle 六态（**没有范围圆**） |
| `States / Editor / 非默认层与 toolsbar` | `A0UG5v` | 全局组置灰 + 浮条；left 三选一、right 眼睛两态 |

口径细节（12 条）在 `design/editor.md` 的「2026-10-31 编辑区（area）面板」一节；
**画稿优先于正文描述**，正文优先于临场发挥。

## 2. W2 —— 渲染（最硬的一波）

### 2.1 目标

一张照片真的按「默认层 + areas」合成；控制点的全图向量匹配出效果；单层照片**语义不变**。

### 2.2 现状（`crates/raybend/src/develop/pipeline.rs`）

```text
render_develop16: 镜头 warp → 降噪 → render_rgb16_with_local_tone（逐像素）→ 锐化 → LUT
逐像素那一段（map_pixel_exact_with / 快路的 LocalPass）：
  每通道 gain(WB×EV) → contrast → highlights → blacks
  → 夹取 → sRGB OETF → base 曲线 → rgb 曲线 → 通道曲线
  → 正向高光中性化（highlights>0 时）→ chroma
```

### 2.3 要做的

1. **层内固定管线**（`specs/editor-area.md` §4.5，照抄顺序，不许各层各写）：
   `WB×EV → 反差 → 高光/黑区 →〔仅默认层：基础曲线〕→ rgb 曲线 → 通道曲线 → 高光中性化 → 色度`。
   * 某层若**既无曲线也无色度 ⇒ 到此为止**，全程留在线性域（省 6 次 OETF）。
2. **链循环**（§4.1）：
   ```rust
   let mut val = base(p);
   for layer in layers_in_order() {          // 默认层 + areas，按 default_index 插进去
       let prev = val;
       let w = weight(layer, prev) * layer.opacity();
       if w <= 0.0 { continue; }
       val = lerp(prev, apply_area(layer, prev), w);
   }
   ```
   **红线：掩膜读 `prev`，绝不读 `val`**（读输出会自我反馈）。
3. **控制点权重**（§4.2）：`d = ‖A(prev) − seed‖₂`（`A = luma, c1=r−g, c2=r−b`，都归一化 0..100 后除以 100），
   `α = V(d / tolerance)`，`V(x) = 0.5^(x·k)`（k 取到 `V(1)≈0.02` 即可，W2 里定一个常数并写进注释）。
   **没有空间项**。
4. **LUT 落点**：从"全局链末端"移到**默认层段的末尾**。因为默认层初始在第 1 位，
   结果就是 **LUT 从"锐化之后"变成"锐化之前"**（§4.7 已登记为**有意变化**）。
   `crates/raybend/src/thumbnail/render.rs::PIPELINE_VERSION` **加一**，否则旧缓存留在旧次序。
5. **局部色调（`dynamicContrast`）**：跟着默认层。引导图按「源图 + 默认层线性参数」预计算
   （现在的 `LocalToneState` 就是这么来的）⇒ 默认层在第 1 位时与今天一致；被移走时是**近似**（写进注释）。
6. **精度契约**（§4.6）：层间 f32 线性、**不落地成图、不夹取**；显示域那一段用 signed 可逆编码
   （`render/creative_lut.rs` 的 `signed_encode/decode` 已有），只有最终输出 clamp。
7. **性能**：掩膜≈0 直接 `continue`；纯影调层不跨域；**可选**：相邻、掩膜恒为 1、只含线性项的全图层
   预折叠成一条 1D 复合表（做了要写单测；不做也能交付）。

### 2.4 顺带要加的参数

四条 `Mask` 参数进 `PARAMS` + `develop-params.json`（**不进前端面板**：`src/features/editor/params.ts`
要按 `target === "mask"` 过滤掉；已经有 `PARAM_GROUPS`，别让它们出现在页签里）：
`areaTolerance`（0..100）、`areaOpacity`（0..100，默认 100）、`areaChroma`（0..100，默认 50）、
`areaLuma`（0..100，默认 50）。

> ⚠ 加进 `PARAMS` 就自动被 `every_develop_param_is_classified_for_crs` 盯上 ⇒ 必须同时在
> `xmp/mapping.rs` 归到 `NOT_MAPPED` 并写一行理由（规格 §13.1 第 4 条）。

### 2.5 验收

* **单层等价**：没有额外层时，渲染结果与改动前**逐点一致**（对着 `map_pixel_exact` 写向量；LUT 次序那一条例外，见下）。
* 层序改变 ⇒ 画面改变；单层开关 / 不透明度生效。
* 控制点：`tolerance` 变大 ⇒ 受影响像素变多；**移动采样位但不改 seed 时，结果与位置无关**
  （这条专门证明"没有空间项"）。
* `cargo test --workspace` 全绿 + 至少 4 条新单测（层序 / 权重单调 / 容差单调 / 位置无关）。

## 3. W3 —— 撤销与持久化

### 3.1 现状

`store/marking.rs::Op` 只有 `DevelopParam { asset_id, param_id, before, after }`、
`DevelopCurve { asset_id, channel, … }`、`DevelopSetting { asset_id, key, … }`。
**一个 `param_id` 现在还能唯一定位**，加了层就不行了；而且**没有任何结构 op**。

### 3.2 要做的

1. `Op::DevelopParam` / `DevelopCurve` 加 `area_id: Option<u32>`：`None` = 默认层
   （**旧历史全是 None，因此旧栈仍可解析**，不要写迁移）。
2. 新增结构 op（都存整段 JSON，照抄 `DevelopCurve` 的手法）：
   `AreaAdd { area }`、`AreaRemove { index, area }`、`AreaMove { from, to }`、
   `AreaRename { id, before, after }`、`AreaToggle { id, before, after }`、`AreaColor { id, before, after }`。
   （控制点的移动 / 容差走 `DevelopParam` 之外的一条 `AreaPoint { id, before: [f32;6], after }` 更省事——二选一，选完写注释。）
3. `marking::apply` 的反向执行要能还原结构（`AreaRemove` 的反向是"插回原位置"，别插到末尾）。
4. `store::develop` 的写入口按层定位：默认层走现有的 `develop_params` / `develop_curves` 表，
   额外层走 `areas` JSON（`set_area_*` 系列，全部经 `save()` 落库，别绕过 `validate()`）。

### 3.3 验收

* 每个结构动作都能撤销/重做，且**位置/名字/颜色原样还原**。
* 旧历史（没有 `area_id`）仍能撤销（`None` = 默认层）。
* 单测：结构 op 的往返、旧 op 兼容、`AreaRemove` 的反向保位置。

## 4. W4 —— 面板与画布（这一波做完才"能上手"）

### 4.1 DTO 先打通（最容易漏）

`src-tauri/src/develop.rs::DevelopStackDto` 现在**没有** `areas` / `defaultIndex`，
`into_stack()` 里写死成空（代码里有注释）。**不加这一步，一次普通编辑就会把层抹掉。**
要一起改：`src-tauri/src/develop.rs`、`src/api/editor.ts` 的类型、`src/api/types.ts`。

### 4.2 前端要做的（按画稿）

| 件 | 落点 | 要点 |
| --- | --- | --- |
| 左列顶部三选一 | `features/editor/toolbar.tsx` + `store.ts::toggleLeft` | 第三个开关「编辑区」，与定稿 / LUT 互斥 |
| **area panel** | **新** `features/editor/area-panel.tsx` | 三种行（默认带锁 / 全图 / 控制点带色点）、拖拽排序（复用 `DragPreview`）、两个新建按钮、32 层上限、双击改名 |
| 层名条 | `features/editor/panels.tsx` + `components/ui/HistogramPanel.tsx` | 直方图 148+28→138+26；条高 22、居中、两头渐隐（`.fade-x-end`，**不用省略号**） |
| 画布标记 | `features/editor/ViewportOverlay.tsx` | 圆片 20 / 白边 2.5 / 内圈 12；引线固定 70px；handle 9px；**屏幕像素固定，位置走 Rust 变换矩阵** |
| 眼睛开关 | `toolbar.tsx` 右段 | **只管标记**显示；`localStorage` 版本化 key；隐藏时**同时关抓取** |
| 非默认层禁用 | `panels.tsx` | 镜头 / 清晰度 / 局部色调 / 色彩管理 / LUT / 基础曲线：灰掉 + 浮条 `当前编辑区域下不可用` |
| 改名弹窗 | 复用 `Dialog` | 双击行触发，30 字符，`默认` 不可改名 |
| 命令注册 | `features/commands/catalog.ts` | 新建控制点 / 新建全图 / 删除层 / 上移 / 下移 / 显示控制点 —— **默认键或写明留空理由**（§2.15） |

### 4.3 撤销节点（§2.21，漏一个撤销就跳步）

新建层 / 删除层 / 拖动排序松手 / 双击改名确认 / 控制点落点 / 拖动圆片改位置松手 /
拖 handle 改容差松手 / 切换启停 / 切换默认层位置 / 换色（若有入口）—— **全部接 `confirmEdit`**。

### 4.4 验收

* 手动：新建控制点 → 点画面 → 出圆片 → 拖动 → 调容差 → 与全图层排序 → 撤销/重做各一步。
* 面板焦点：换照片回默认层；删当前层选下一层；关面板再开保留当前层。
* 32 层时两个新建按钮变灰 + 带数字的浮条。

## 5. W5 —— 周边

| 面 | 要做什么 |
| --- | --- |
| 预设 | `src/lib/presets.ts` 与 Rust 侧：**非默认层不捕获 Global 参数**；`Mask` 项一律不捕获；套预设不会新建 / 删除层 |
| XMP | `crs:` **只写真正全局项**；层只进 `rb:profileJson`（自动跟随）；`specs/xmp-sidecar.md` §5.1 补一句"多层怎么表达、旧版怎么读" |
| auto_adjust | `AutoAdjustBaseline` 只作用于默认层（`apply` 时写进 `params`，不要写进 `areas[0]` 的语义错觉） |
| 缩略图 | 确认层循环只在 `render_develop16` 一处（缩略图 / 预览 / 导出三条路都走它） |
| 缓存 | `signature()` 已含层；检查 `latest-<hash>` 与 `-full-` 两档都跟着换 |

## 6. 贯穿全过程的口径（速查，违反任何一条都要返工）

1. **层内只允许逐像素操作**（`AGENTS.md` §2.27）—— 镜头 / 降噪 / 锐化 / 局部色调 / LUT / 色彩管理 / 几何都是全局。
2. **合成不落地**：层间只传 f32 线性，绝不写回 u16 / 8-bit 中间图。
3. **掩膜读本层输入**，不读本层输出。
4. **控制点没有空间项**（`tolerance` 是向量容差）—— 不画范围圆。
5. **列表自上而下 = 计算顺序**，新增追加在末尾；说"第 N 层 / 先·后"，不说"上/下"。
6. **退化形状逐字节不变**：新字段一律 `skip_serializing_if` 退化掉。
7. **默认层不是 `areas[0]`**，是栈自己；位置在 `default_index`。
8. **两个版本号**：`profile_schema_version()` 按内容选；`PROFILE_SCHEMA_VERSION` 是读取上限，加版本时**两个一起动**。
9. **参数表三处同步**（Rust / JSON / 前端读同一份），加参数就归 `target`，并归 XMP 的 `SCALARS` 或 `NOT_MAPPED`。
10. **不做封闭控制点**（方案存档 `memory/FUTURE.md` D14），也不给 `Point` 加任何"封闭"开关。

## 7. 已知陷阱（都是踩过的）

| # | 坑 | 后果 |
| --- | --- | --- |
| 1 | 只改 `profile_schema_version()` 忘了 `PROFILE_SCHEMA_VERSION` | 写进去的定稿读不回来（W1 已经踩过一次，已修） |
| 2 | `DevelopStackDto` 不带 `areas` | 任何一次普通编辑把层抹掉 |
| 3 | 忘了 `PIPELINE_VERSION` | 旧缓存停留在旧次序 ⇒ 毒缓存 |
| 4 | 给 `DevelopStack` 加字段却忘了 `skip_serializing_if` | 所有旧定稿 `profile_hash` 对不上，判为「配置指纹不一致」 |
| 5 | 在 `prune_empty_stack` 的 SQL 里漏了新列 | 空栈删不掉（或反过来，有层的栈被删） |
| 6 | 前端参数面板直接按 `PARAM_GROUPS` 渲染 | 四条 `Mask` 参数冒出来变成无消费者的滑杆 |
| 7 | 掩膜读 `val` 而不是 `prev` | 拖动时选区缓慢爬行、不可复现 |
| 8 | 用绝对定位的渐变块做层名渐隐（Pencil 里试过） | 被文字盖住；实现侧直接用 `.fade-x-end` 遮罩 |
| 9 | 工作树里有**别的会话**的改动（LUT 链、窗口恢复、弹窗描边等） | **不要回滚它们**；自己每波提交一次，提交信息中文 `<type>: <subject>` |
| 10 | 构建 / 发布进行中改工作树 | 会让校验中断，先看 `.release/build.lock` |

## 8. 质量门（每波结束都跑，别攒到最后）

```bash
cargo check --workspace
cargo test --workspace          # ❗必须是 --workspace，-p 会漏掉别的 crate 的测试目标
pnpm typecheck && pnpm test
pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n
pnpm build                      # 动了前端就要跑；产物要嵌进 exe
```

W2 之后额外跑一次 `cargo test --workspace` 里的 xmp 组（`cargo test -p raybend xmp`）。

## 9. 什么算做完（交付给崔总测试）

**功能面**：三种层可用；控制点全图匹配生效（容差可调）；层可增删 / 排序 / 改名 / 启停；
眼睛开关；非默认层右栏置灰；撤销重做覆盖全部动作。

**数据面**：定稿 / 预设 / XMP 三条路都带层且能读回；旧数据（没有层）打开后画面与之前一致；
`profile_hash` 与 `signature()` 的退化形状不变。

**交付物**：

1. 全部质量门绿（把命令原文与结果写进实施记录，**别写"全绿"了事**）；
2. `implementations/2026-11-XX_editor-area-w{2..6}.md` 每波一份；
3. `specs/editor-area.md` 的波次表与决策账本同步（做完的勾掉、变了的改口径）；
4. **给崔总出测试产物用 `pnpm debug:win`**（不是 `release`，release 会顶版本号且 15 分钟起）；
5. 交付说明里必须分清「单元测试已验证」与「未经人类验证」（GUI / 真机 / 性能体感 / 色彩正确性都属后者）。

**崔总要测的**（写进交付说明）：新建三个控制点各自调色 → 看是否只有预期区域变；调容差看渐变；
拖动采样位看是否只换种子；与全图层排序看 LUT 前后差异；撤销十步；存定稿 → 切 SOOC → 切回来；
存预设 → 套到另一张；导出看结果是否与编辑器一致。

## 10. 明确不做

* **封闭控制点**（`memory/FUTURE.md` D14）；
* 蒙版 / 智能蒙版 / 画笔；
* 层不透明度 UI（字段保留、界面不给）；
* 控制点的"显示影响面"；
* 层复制 / 粘贴（`memory/FUTURE.md` D12，「有人给我钱了再做」）；
* 三路色彩圆盘（`memory/FUTURE.md` D11）。
