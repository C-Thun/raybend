/* @refresh reload */
import { lazy } from "solid-js";
import { render } from "solid-js/web";
import { Route, Router } from "@solidjs/router";
import App from "./App";
import "./index.css";
/*
 * CJK 字体包从 **JS** 引入（而不是在 `index.css` / `styles/fonts.css` 里 `@import`）：
 * 编译期有个插件要给它的每个 `@font-face` 补 metrics 覆盖（`vite.config.ts` 的
 * `cjkMetricsOverride`，治「中文上飘」），而 CSS 的 `@import` 由 postcss 直接读盘内联、
 * **不过插件管线** —— 那样补好的东西进不了浏览器真正生效的那张表（实测踩过）。
 */
import "@fontsource-variable/noto-sans-sc/wght.css";
import { installEscapeBlur } from "./lib/dom-focus.ts";
import { hydrateLocale } from "./i18n/index.ts";

/**
 * 组件陈列室（`src/dev/`）**只在开发期注册**，而且 `import()` 必须留在
 * **模块顶层的三元里**。两个坑都实测过：
 *
 *   1. 静态 import → 组件连同 Ark UI 一起进产物
 *   2. 把 `lazy(() => import(…))` 写进 JSX 内部（哪怕是 `import.meta.env.DEV ? <Route …/> : null`）
 *      也没用：Solid 编译器会把 JSX 子节点包成 `get children()` / `$memo()`,
 *      `import()` 就落在了一个被保留的函式体里，Rollup 看得见、摇不掉 ——
 *      实测生产构建里照样多出一个 **254KB 的 `KitchenSink-*.js`**
 *
 * 只有写成下面这种「模块级三元」时，生产构建才会把 `DEV` 替成 `false`、
 * 把整个 `import()` 摇掉（验证方式：`pnpm build` 后 `ls dist/assets/*.js` 只有入口一个）。
 */
/*
 * 「按 Esc 一律先清焦点」——一处收口，不用在每个弹窗里各写一遍
 * （根因与顺序纪律见 `lib/dom-focus.ts`）。热重载时先卸再装，免得越挂越多。
 */
const disposeEscapeBlur = installEscapeBlur();
/*
 * 语言必须在**首次渲染前**读回来：它是设备级偏好（localStorage），
 * 放渲染后再读会先闪一帧中文再跳成英文。
 */
hydrateLocale();
import.meta.hot?.dispose(() => disposeEscapeBlur());

const KitchenSink = import.meta.env.DEV
  ? lazy(() => import("./dev/KitchenSink.tsx"))
  : undefined;

render(
  () => (
    <Router>
      {/*
        ⚠️ 路由**必须用 JSX 子节点形式**（`<Route …/>`）而不是把 `RouteDefinition[]`
        数组喂给 `<Router>` —— 实测数组形式下路由一条都不匹配，页面**静默空白**，
        控制台连一句报错都没有（`@solidjs/router` 1.0 的 `createBranches` 在那种输入下不出声）。
      */}
      {KitchenSink ? (
        <Route path="/dev/kitchen-sink" component={KitchenSink} />
      ) : null}

      {/*
        兜底路由用 `path="*"` 而不是 `path="/"` 是刻意的：桌面应用的页面来自
        `tauri://localhost/` 或开发期的 `http://localhost:1420/`，
        兜底路由可以避免 URL 形态差异导致白屏。
      */}
      <Route path="*" component={App} />
    </Router>
  ),
  document.getElementById("root") as HTMLElement,
);
