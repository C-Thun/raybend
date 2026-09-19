# import 与 browse 统一 tiles / film / view（三态 + 对比 + Tab）+ Tab 第③态纠正

完成时间：2026-09-19 14:39:50 CST

> 人类 2026-09-19 的两批要求：① Tab 第③态应当是 `view only`；
> ② 两侧统一支持 tiles / film / view、多选对比与 Tab 切换，且**以 import 的 tiles 为基准**。

## 1. 三态纠正（第③态）

原来第③态是「关胶片带、左右回来」= `view+左右` ✗ —— **没有这种组合**。现在：

| 状态 | 显示 |
| --- | --- |
| `default` | film + 左右两栏（有胶片带） |
| `film-only` | film（收起左右） |
| `view-only` | **只看图**：左右与胶片带**一起收** |

状态名改成「说显示成什么样」（旧的 `no-sides` / `no-film` 说的是「关了哪个」，语义一改就歧义）；
「认不出来的值」仍按默认态处理（与 `nextChrome` 同口径：宁可多显示，不要凭空把界面收没了）。

## 2. 统一（复用优先）

| 能力 | 之前 | 现在 |
| --- | --- | --- |
| 三态循环 | `features/browse/chrome.ts` | `lib/viewer-chrome.ts`（两侧共用） |
| 对比规则 | `features/browse/compare.ts` | `lib/viewer-compare.ts`（结构类型 + 泛型） |
| 键盘守卫 | `features/browse/keys.ts` | `lib/viewer-keys.ts` |
| 胶片带 | `features/browse/FilmStrip.tsx`（绑 browse store） | `components/ui/viewer/FilmStrip.tsx`（`selectedIds` + `onSelect(id, mode)`） |
| 对比视图 | `features/browse/CompareView.tsx` | `components/ui/viewer/CompareView.tsx`（本来只用共享 viewer store） |
| 三态 / 对比 / Tab | 只有 browse 有 | import 也有（同一批组件） |

**import tiles 零改动**（人类硬约束，附证据）：`git diff src/features/photo-grid/ src/lib/tile-flow.ts`
为空；中列默认仍渲染同一个 `PhotoGrid`，只多了一层 `<Show>` 包裹与 `relative`。

## 3. 顺手修掉的两个真问题

- **`lib → ui` 分层违规**：对比数学原本 import UI 层的照片类型 → 改成自己声明结构类型
  `ComparablePhoto`（口径与 `ViewerPhoto.natural` **完全一致**：可选、非 null），
  并把 `compareFrames<T>` 泛型化 —— 帧里装**原始对象**，所以 `fileName` / `path` 不丢。
- **`FilmStrip` 里的 browse 专属假设**：`onSelect` 的 id 原来是 `number`（browse 的 id 是数字），
  而 import 用路径 → 改成 `string`（与 `ViewerPhoto.id` 一致），数字转换挪回各调用方。

## 4. 同批的另一处修复

右下角缩放控件的**触发区**：原来是 `200×200` 的正方角，而控件最长约 350px
（三键 + 百分比 + 文件名）且离角 12px —— 鼠标在条的左段往外一偏，整条当场隐掉。
改成「条的最大宽度 + 周围一圈」= **420×96**；纯几何，不引入测量与 observer
（每条 pointermove 测 rect 会白白触发布局）。

## 5. 验证

- `tsc` 0；`pnpm test` **662 全过**；`lint:arch` / `lint:colors` / `lint:i18n` 通过；`build` 通过
- `smoke:ui` problems: `[]`；`check:browse` 通过 —— **它当场抓到了三态语义回归**
  （旧断言期望第③态左右栏 = 2），期望值已随新语义更新，这正是冒烟该有的作用
- import tiles 零改动：`git diff` 为空（见 §2）

## 6. 遗留

- 胶片带的**标记与外形**仍是自绘的一份（竖图问题已修）：要彻底单一实现，
  需要给 `Tile` 加一个「不要文件名条」的面，再让胶片带用它。
- import 侧暂未接「双击进看图」（browse 有；`Enter` 已可用）。
