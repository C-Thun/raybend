/**
 * `TilesShell` —— **tiles 视图的外框**：上面是网格/看图区，下面是那条状态条。
 *
 * 人类 2026-09-19：「我们沟通语境下的 tiles，就是包括下面状态条的，因为在业务层面就是一个
 * **不可分割的整体**……如果之前是分开的这次就要封装起来供外框架整体引用，配置也是统一传进去」。
 *
 * 于是两个工作区都用它拼中列：
 *
 * ```tsx
 * <TilesShell bar={{ count, selectedCount, dir, fileName, byTime, … }}>
 *   <PhotoGrid … />        // 或 <BrowseGrid … />
 * </TilesShell>
 * ```
 *
 * 分工：
 *   * **网格怎么画**由调用方给（两侧的数据源本来就不同：导入是源目录清单，浏览是分页窗口）；
 *   * **底下那条长什么样**由 `bar` 配置决定 —— 两侧用的是同一个组件、同一份口径，
 *     差异只剩「传不传排序」这种显式开关。
 *
 * `bar` 不给就是不要状态条（看图态那条由别处替代 —— 浏览侧是胶片带、导入侧是看图件）。
 */

import { createSignal, Show, untrack, type JSX } from "solid-js";
import { TilesControlBar, type TilesControlBarProps } from "./TilesControlBar.tsx";
import { TilesFitRequestContext } from "./fit.ts";

export interface TilesShellProps {
  /** 网格 / 看图区 */
  children: JSX.Element;
  /** 状态条配置；`null` / 不给 = 不显示状态条 */
  bar?: TilesControlBarProps | null;
  class?: string;
}

export function TilesShell(props: TilesShellProps): JSX.Element {
  const [fitRequest, setFitRequest] = createSignal(0);
  /*
   * 网格 / 看图件**只创建一次**（untrack）—— 与 `BarFrame` 同一条理由（`AGENTS.md` §2.17）：
   * `{props.children}` 被编译成 `insert(el, () => props.children)`（一个 render effect），
   * 于是「创建 children 期间被读到的信号」一变，整棵子树就会被重建。
   * 网格被重建 = 滚动位置、选择、已铺出的缩略图全丢 —— 正是 §11.4 里
   * 「PhotoGrid 只能隐藏、不能卸载」那条红线要防的事。
   */
  const children = untrack(() => props.children);
  return (
    <TilesFitRequestContext.Provider value={fitRequest}>
      <div
        class={["relative flex min-h-0 flex-1 flex-col", props.class ?? ""]
          .filter(Boolean)
          .join(" ")}
        data-tiles-shell
      >
        {children}
        {/*
          when 只看「有没有 bar」，不看配置对象身份：拖动滑杆时 tileStep 每次变化都会
          产生新对象，但这颗 TilesControlBar 必须保持同一个 DOM / Ark Slider 实例，
          pointer capture 才不会在第一格之后被销毁。
        */}
        <Show when={props.bar !== null && props.bar !== undefined}>
          <TilesControlBar
            config={() => props.bar as TilesControlBarProps}
            onFitRow={() => setFitRequest((request) => request + 1)}
          />
        </Show>
      </div>
    </TilesFitRequestContext.Provider>
  );
}
