# 拖动采样点即重新取样；自绘光标（取样位 / 旋转）

> 完成时间：2026-11-01 23:21:33

## 1. `Ctrl` 拖采样点 = 换地方取样（不再是"位置变了、种子不动"）

**旧行为**：`dragPoint` 只写 `x/y`，种子永远停在第一次落点那组数 —— 拖到别处画面一点不变。

**新行为**（崔总 2026-11-01 定）：松手后按**新位置**、按该层在**行序**里的下标重新走一次
`develop_area_seed` → `adoptAreaSeed` → `confirmEdit`（位置 + 种子同一次撤销）。

* `src/features/editor/area-markers.tsx`：`AreaMarkersProps.onResample(id)`；`dragPoint` 的
  `end` 有回调就交给它（否则退回 `onCommit`，只存位置）。
* `src/workspaces/editor/EditorWorkspace.tsx::resampleAreaPoint`：RAW 基准下才取样（与落点同口径）；
  取样失败走统一消息通道，**并把已改的位置照样落库**（否则画面与库对不上）。
* 取样只合成它**前面**的层（`compose_linear_upto`），这一层自己的旧种子不参与，所以无需先清种子。
* `specs/editor-area.md` §3.3 同步改口（旧文写的是"移动时种子不重取"）。

## 2. 光标：不再一律用系统关键字

| 场景 | 之前 | 现在 |
| --- | --- | --- |
| 圆片按住 `Ctrl`（可移动采样位） | `grab` | 自绘「圆环 + 四向箭头」`rb-cursor-sample` |
| 旋转工具的画布 | `cursor-crosshair` | 自绘「垂直向上箭头」`rb-cursor-rotate`（2026-11-02 崔总指定：标平行线那套软件的光标；十字表示定位、圆弧表示逆转，都不对） |
| 裁切 / 武装落点 | `cursor-crosshair` | 不变（十字在这里是对的） |

实现：`src/styles/tokens.css` 末尾 `--cursor-sample` / `--cursor-rotate`（内联 SVG data URI，
双色描边保证亮/暗照片上都读得清，hotspot 在中心 12 12）＋两个工具类；组件只加类名。
cursor 图片**不能**用 `currentColor`，所以色值烘进 data URI —— 放在 `tokens.css` 是唯一符合
`lint:colors` 的位置。

`area-markers.tsx` 里用 window 级 `keydown/keyup/blur` 跟踪 `Ctrl`（焦点可能在视口或按钮上）。

## 3. 验证

```bash
pnpm typecheck   # 通过
pnpm test        # 1382 passed / 0 failed
pnpm lint:colors # 通过（光标 SVG 在 tokens.css 的允许清单内）
pnpm build       # 通过
```

## 4. 未做 / 待真机

* 光标外观与手感（尤其深色照片上的白描边）需要真机看；
* 取样是 1024 长边的线性源，拖动松手会多一次几十毫秒的解码 —— 真机上若觉迟滞，再考虑拖动中只跟随、松手取样的过渡提示。
