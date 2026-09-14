/* @refresh reload */
import { render } from "solid-js/web";
import { Route, Router } from "@solidjs/router";
import App from "./App";
import "./index.css";

/*
 * 路由：M0 阶段只有一条兜底路由。
 *
 * 用 `path="*"` 而不是 `path="/"` 是刻意的：桌面应用的页面来自
 * `tauri://localhost/` 或开发期的 `http://localhost:1420/`，
 * 兜底路由可以避免 URL 形态差异导致白屏。
 *
 * 是否需要真正的 URL 语义（多个工作区各自可寻址）由 UI 设计阶段定夺；
 * 若最终不需要，此依赖可以移除。
 */
render(
  () => (
    <Router>
      <Route path="*" component={App} />
    </Router>
  ),
  document.getElementById("root") as HTMLElement,
);
