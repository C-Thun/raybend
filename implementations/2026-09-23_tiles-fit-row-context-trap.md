# 修好缩放杆，却按死了「横向适合窗口」——Solid children 的另一面

完成时间：2026-09-23 11:40:33 CST

## 1. 起因

人类 2026-09-23 报：「不是，你修好了拖动缩放杆，但怎么把自动宽度的按钮功能改坏了啊？这两者不可兼得吗？」

**能兼得** —— 是同一个手法有两面，我第一轮只治了 A 面，把 B 面踩了。

## 2. 根因：`untrack` 的位置错了

上一轮（`implementations/2026-09-23_tiles-zoom-slider-drag-rebuild.md`）为了治「children 被重建」，
我在 `TilesShell` 里写成了：

```tsx
const children = untrack(() => props.children);   // ❌ 在组件 body 里求值
...
<TilesFitRequestContext.Provider value={fitRequest}>
  <div ...>{children}</div>
```

问题：Solid 的 `useContext` 走的是**「创建时的 owner 链」**。children 在 body 里就被造出来了，
而 `<TilesFitRequestContext.Provider>` 是**之后**才在返回的 JSX 里创建的 —— 于是 `PhotoGrid` 的

```tsx
const fitRequest = useTilesFitRequest();          // → undefined
createEffect(on(() => fitRequest?.() ?? 0, ...)); // → 永远 0，effect 从不触发
```

拿到 `undefined`，**「横向适合窗口」按钮按下去毫无反应**。拖动那条线倒是因为 `BarFrame` 的
`untrack` 没动而照旧正常 —— 两个毛病互不相关，所以表现为「修一个坏一个」。

## 3. 修复

**在插入点就地 untrack**（`{untrack(() => props.children)}`，就写在 JSX 里那一行）：

- children 仍然只创建一次（A 面治住了：那次创建发生在 `insert()` 的 render effect 里，
  而 untrack 让该 effect 不再依赖「创建期读到的信号」）；
- children 仍在 Provider 的 owner 链里（B 面治住了：`{...}` 那一行在 Provider 之内求值）。

| 文件 | 改动 |
| --- | --- |
| `src/components/ui/tiles/TilesShell.tsx` | body 里的 `const children = untrack(...)` 删掉，改成 JSX 插入点的 `{untrack(() => props.children)}`；注释里写明「为什么不能提前求值」 |
| `src/components/ui/tiles/TilesControlBar.tsx` | `BarFrame` 同样改成插入点 untrack（它现在没有 Provider，但同一个壳组件迟早会有 —— 一处规矩，别留特例） |
| `src/components/ui/tiles/TilesControlBar.tsx` | 「横向适合窗口」按钮加稳定测试钩子 `data-tiles-fit-row`（与 `data-tiles-control-bar` / `data-tile-info` / `data-sort` 同一条规矩） |
| `scripts/check-browse-boot.mjs` | 新增「横向适合窗口」回归（见下） |
| `AGENTS.md` | §2.17 改成**两面并列**的版本（人类要求：「那个提示要并行说明，这两者的教训」）；§11.4 同步 |

## 4. 回归怎么写才不会空跑

「点一下按钮，档位变了没有」是不够的：**上一轮如果已经把档位调成了合适值，什么都不做也能蒙对**。
所以这条断言分三步：

1. **先把档位挪开** —— 点缩放轨道左端（= 最小档 128px），并断言档位确实到了最小
   （顺带把「点轨道能改值」也钉住）；
2. 点「横向适合窗口」，然后量几何：**首行宽度**（首格左边 → 末格右边）应当等于
   网格可用宽度（`scroller.clientWidth - padding`，±1.5px）。判据与 gap / 密度令牌无关 ——
   量的是「铺满」这件事实本身。档位被夹到两端（0 / 16）或首行不满格时跳过；
3. **点轨道还原档位**，别把状态留给后面那些几何 / 信息条断言。

首轮写这条断言时量错了 `gap`（去读行元素的 `computedStyle.gap`，读到 0；真值来自密度令牌
`--gap` = 3 / 6px），于是把一次**分毫不差**的铺满（3 列 × 285 + 2 × 3 = 861 = 容器宽）
误报成「差 2.0px」。改成首行宽度比对后不再依赖 gap。

## 5. 验证方式（Agent 侧冒烟）

**探针页（真实 TilesShell + TilesControlBar + Slider，临时页面，用完即删）**：

| 状态 | fit 上下文 | 点按钮 | 缩放杆 |
| --- | --- | --- | --- |
| 正解（插入点 untrack） | `yes` | `request 0 → 1` ✓ | 4 → 16 一路跟手、节点身份不变 ✓ |
| 临时注入 B 面 bug（body 里 untrack） | `no` | `request -1 → -1` ✗ | 照旧正常 ✓ |

反面验证说明两件事：① 探针抓得到这个 bug；② **两面确实互不相关**（治 A 不会自动治 B）。

**真应用（`pnpm check:browse`，假后端 + 真鼠标）**：在 app 还能起来的那次运行里，
量到「点适合窗口后 3 列 × 285px + 2 × 3px gap = 861px = 容器宽」—— 接线是通的、铺满是准的。
本轮新加的三步断言（挪开 → 点 → 还原）在最后几次运行里因 **app 起不来**（见遗留）没能复量。

**门禁**：`pnpm typecheck` 0 错 / `pnpm test` 全过 / 三条 lint / `pnpm build` 全绿。

## 6. 遗留

1. **树里另一个工作单元的在途改动当前把 app 弄得起不来**（`check:browse` 报「没找到『浏览』标签」、
   左列空白、`photosFromSource is not defined` 等），本次新增的 fit 断言因此量不到网格几何
   （报「点了『横向适合窗口』之后量不到网格几何」）—— **不是产品问题**，等那批落定后重跑即可复量。
2. `AGENTS.md` 里我的两处改动**暂未提交**：那个文件夹着另一工作单元的在途改动，提交会把他们的
   改动一起卷进来。等那批落定后一并提交。
3. 同类壳组件 `shell/TitleBar.tsx` / `shell/ToolsBar.tsx` / `dev/KitchenSink.tsx` 还没加插入点 untrack
   （那两个 `shell/` 文件正被另一个工作单元改，没动）；§2.17 的规矩已覆盖它们。
