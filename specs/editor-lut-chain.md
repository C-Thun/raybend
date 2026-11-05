# editor 多层 LUT（应用层 / LUT chain）

> 2026-10-31 开工。画稿：`design/editor.pen` → 主稿 `P1JiB` 左列 + `Components / Editor / LUT 应用层`（`pj0oo`）；
> 规格说明：`design/editor.md` §3.1.3。需求原文与审阅：`todos/2026-10-31-lut-multilayer-ui.md`。
> 前置（LUT 上 GPU）：`implementations/2026-10-31_lut-on-gpu.md`。

## 0. 一句话

一支 LUT 变成**一条有序的 LUT 链**：列表点选换第 1 层（底层）、右上角圆圈加/减层、
每层有独立开关与强度、可拖动排序；CPU 与 GPU 走同一份语义，issue / preset / XMP / 撤销全部带上整条链。

## 1. 语义（唯一口径）

```text
链 = [ layer1(底层), layer2, ... ]            自上而下 = 逐层处理顺序
应用 = master_on && 至少一层 enabled
像素 = layer_n( ... layer_2( layer_1( 原始像素 ) ) )
单层作用 = mix(输入, Lut::eval_extended(输入), strength)     strength = 1 时就是 eval_extended
```

| 项 | 定义 |
| --- | --- |
| `strength` | `0..=1`，默认 `1.0`（100%）。在**显示参考的编码域**里混合（与 LUT 求值同域） |
| `enabled`（层） | `false` = 该层旁路（跳过），不影响其它层。**强度一写入就改写它**：0% ⇒ 关，>0% ⇒ 开（只动这一层，不碰总开关） |
| 总开关 | `DevelopStack.lut_enabled`：关掉时整条链不参与渲染，**链保留** |
| 中心/顺序 | 链有序；顺序、每层开关与强度都进 `DevelopStack::signature()` |
| 重复 | **不允许**同一 `id` 出现两次：点已在链中的 tile = 把它移到第 1 层 |
| 上限 | **8 层**（GPU 一趟 pass 内串联；超过时 UI 禁用「加入」并提示） |
| 空链 | 与改动前完全一致（无 LUT） |
| 在 area 链里的位置 | LUT 挂在 **`默认` 层**上（`AreaKind::Default`），随它的位置执行（`specs/editor-area.md` §4.3）；LUT 数据仍在 `lut_chain`，层本身不存 LUT |
| 替换底层 | 新底层用默认 `enabled=true, strength=1.0`，不继承旧值 |
| 删除第 1 层 | 下一层顶上（有序表自然结果，无特判） |
| 缺文件 | 该层跳过并给可见提示；其余层照常 |

## 2. 数据模型

```rust
// crates/raybend/src/store/develop.rs
pub struct LutLayer { pub id: String, pub enabled: bool, pub strength: f32 }  // 默认 true / 1.0

pub struct DevelopStack {
    pub lut_chain: Vec<LutLayer>,      // 新真相源（空 = 无 LUT）
    pub lut_enabled: Option<bool>,     // 总开关（语义不变：None/false = 关）
    pub lut_id: Option<String>,        // **只读兼容**：旧数据/旧 XMP 的单支 id；反序列化时并入链
    // …
}
```

- **兼容入口只有一处**：`DevelopStack` 用 `#[serde(from = "DevelopStackWire")]`，
  `From<DevelopStackWire>` 里做「`lut_chain` 为空且 `lut_id` 有值 → 单层」的归一化。
  DB 装载、XMP `rb:profileJson`、issue 快照、preset 全部走同一个反序列化，不会漏。
- 序列化只写 `lut_chain`（`lut_id` 仅在有值时写，供旧侧车判断；新数据为空）。
- `validate()`：每层 id 非空、≤128、不含路径分隔符；`strength` 有限且 `0..=1`；层数 ≤ 8；同 id 不重复。
- `signature()`：把链按顺序拼进指纹（`id|enabled|strength`），强度用定点小数（`{:.5}`）。
- `is_empty()` / `len()`：以链与总开关为准。
- **撤销的 Setting**：`LutId` → `LutChain`（值为链的 JSON）+ 既有的 `LutEnabled`（总开关）。
- DB：`catalog` 迁移新增 `lut_chain TEXT`（NULL = 未设置）；`prune_empty_stack` 改看 `lut_chain`。

## 3. 渲染

### 3.1 CPU（导出 / 缩略图 / 直方图 / 慢速路）

`DevelopStages.lut: Option<&Lut>` → `lut: &'a [AppliedLut<'a>]`（`AppliedLut { lut, strength }`，空切片 = 无）。

- 显示域（u16）：逐层 `Lut::apply_rgb(rgb, strength)`（现有实现已经是 `eval` + 混合，语义不变）。
- 工作域（f32）：逐层 `x + (eval_extended(x) - x) * strength`；`strength == 1` 时**逐位等于**现在的 `eval_extended`。
- 单层时逐位等价于改动前（验收要求）。

### 3.2 GPU（编辑器快速路）

**一张纹理装整条链**（不新开绑定组、不按层开纹理）：

- **3D 图集**：`texture_3d<f32>`，每个层占一段 Z（slot = `max_n + 2` 片，前后各一片是边界复制），
  X/Y 也各留 1 texel 边界（复制），于是**层与层之间不会被三线性插值串味**。
  每层按自己的 `n` 求值：`iz = slot_start + 1 + zc*(n-1)`。
- **shaper 数组**：`texture_2d_array<f32>`（layer = 层序号），每层长度不从纹理尺寸推断，放 uniform。
- **uniform**：`count`、每层 `(offset_z, n, strength, enabled)`、`has_shaper`/`shaper_len`、总开关。
- 只有 `strength`/开关变化时**只改 uniform**；增删层/换 LUT/改顺序才重建图集（低频动作）。
- 求值与 CPU 同序：编码域 `signed_encode → 逐层 → signed_decode`，超域残差按层保留。

### 3.3 解析入口

`src-tauri`：`resolve_chain(app, stack) -> Result<Vec<AppliedLutOwned>>`
（`vcc; id → Arc<Lut>` 走既有 `lut_cache`，按整条链的 id 序列做键）。
thumbs / export / issues / sidecar / 编辑器会话全部改用它（不允许各自拼一遍）。

## 4. 界面（画稿即规格，见 `design/editor.md` §3.1.3）

- 应用层列表：行高 54；缩略图 56×42（占两层）；上层名称 + 单层开关；下层强度拉杆 + 数值 + 删除位（14×14 常占位，指向才画图标）。
- 两条通道：点 tile = 换/移到第 1 层；点右上角圆圈 = 加/减层（已在链中常显实心勾）。
- 拖动整行排序：**指针手势**（共享 `lib/pointer-drag.ts`，阈值 4px、出窗取消），
  整行本体作幽灵跟随光标（`surface-layer` + 主色细描边），落点槽位画 `state-selected` 占位块，
  其它行 140ms 过渡让位；几何规则在 `lib/row-drag-sort.ts`（有单测），手势与视觉件是共享的
  `components/ui/RowDragSort.tsx`（与编辑区层的整行拖动**同一份**）。松手才 confirm。
- 强度拉杆**每一次写入值**都同步定该层开关：0% = 关、>0% = 开；双击还原 100% 也走同一条（会开层）。
  松手（`lib/slider-gesture.ts` 的真实手势收尾）只多记一个 confirm 节点，不再承担判定 ——
  判定一旦依赖手势收尾，只要有一回没收到（指针被别处捕获、值从程序侧写入、组件重挂），
  就会留下「开着但 0%」的层（2026-11-01 崔总：这套联动曾整个失效）。
  **不能用** Zag 的 `onValueChangeEnd` 当收尾：它在外部 `value` 回灌时也会派发（2026-10-31）。
- **不存在「开关开着但强度 0%」的层**（2026-11-01 崔总定）：
  把 0% 的那层点开关打开时，强度**直接给回 100%**；非 0% 的层开关只改开/关、强度一点不动。
  与「强度 0% 自动关层」是同一条规则的两个方向（都在**写入的同一步**定，不分拖动/松手）。
  这条联动**不受顶部总开关影响**：总开关关着时它照常生效，但整条链依旧不参与渲染。
- 空链：整块不存在。链长到 8：列表里的圆圈变为禁用。
- 命令与热键：本功能没有新的可独立触发的全局动作（都在面板内），按 §2.15 在实施记录写明理由。

## 5. 撤销 / 定稿 / 预设 / XMP

| 面 | 做什么 |
| --- | --- |
| 撤销 | 每次链变更（点选换底、圈选加/删、删行、拖动松手、强度松手、单层开关、总开关）都是一个 confirm 节点；`Op::DevelopSetting("lutChain")` 存整链 JSON |
| 定稿 | issue 快照存整链；缓存签名含链；封面用整链套样片 |
| 预设 | 存整链；旧预设（单支）按 §2 的归一化读入。**应用时是追加，不是替换**：预设的层贴到当前链末尾，按「从上向下」去重（相同 LUT 保留最上层），超出 8 层的部分不加入并提示一次；总开关从空链起步时听预设、已有链时不会被打预设关掉；**预设里 LUT 大类为空 = 不动当前链**（预设整体是加法，要清空只能在面板逐层删）（崔总 2026-10-31） |
| XMP | `rb:profileJson` 自动带整链；`crs:` 侧 LUT 仍在 `NOT_MAPPED`（不新增映射）；`specs/xmp-sidecar.md` §5.1 补一句 |

## 6. 验收

1. 单层链的渲染与改动前**逐位一致**（CPU 与 GPU 各一条）。
2. 链顺序改变 → 画面改变；单层开关/强度生效。
3. 重排 / 改强度 / 加删层后缓存失效（`signature` 覆盖链）。
4. issue / preset / XMP 三条路都能存回整条链；旧数据（`lut_id` 单支）读回等价于单层链。
5. 8 层上限、重复 id、非法强度、空 id 都被校验拦住。
6. GPU 与 CPU 在 1–3 层链上逐点差 ≤ 3/255（离屏对照）。
