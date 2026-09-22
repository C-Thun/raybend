# 胶片带三点缩放把手
完成时间：2026-09-21 00:44:31 CST

## 改动范围

- 在 import / browse 共用的 `FilmStrip` 顶部、照片 view 与胶片带结合处增加 8px 高的三点拖拉条。
- 向上拖动放大、向下拖动缩小；鼠标位移映射到既有 17 档真实 tile 高度并就近吸附，
  没有增加第二份自由高度状态。
- 拖拉条复用项目唯一的 `SplitHandle`，因此沿用 pointer capture、按帧节流、触摸手势隔离、
  拖动状态和 hover 反馈。
- 支持键盘 `↑` / `↓` 逐档调整，`Home` / `End` 跳到最小 / 最大档，并提供当前档位与像素高度的
  无障碍读数。
- `Ctrl + 滚轮` 保留为补充操作；两种入口都写入同一个受控档位，因此继续继承 import / browse
  分域、像素锚定与停止 2 秒后写 `app.db` 的防抖逻辑。
- Pencil 画布中的普通 film 与 compare film 已同步，并更新配套设计和架构文档；当前 Pencil 会话
  尚未把内存画布保存回本地 `design/browse.pen`，需人类在 Pencil 中执行一次保存。

## 关键决策

1. 不创建新的 resizer 组件：这是标准的横向 `SplitHandle`，调用方只负责把 `deltaY` 转为档位。
2. 位移转换读取 `FILM_STRIP_TILE_HEIGHT_STEPS` 的真实高度并寻找最近档，而不是在交互里硬编码
   9px；以后即使 17 档改成非等距，把手也不会与画面尺寸漂移。
3. 正好位于两档中点时顺拖动方向跨档，使吸附点没有方向性迟滞；越界与非有限输入统一夹取。
4. 这是一项区域内连续调整手势，不适合注册成命令面板动作或可改快捷键；键盘操作只在把手
   已聚焦时生效，避免占用全局按键。

## 验证

- `pnpm typecheck`：通过。
- `pnpm test`：通过，73 个测试文件全部通过；新增拖动方向、半档吸附、上下限与非法输入覆盖。
- `pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：通过。
- `pnpm build`：通过；仅有既有的大 chunk 提示。
- `pnpm smoke:ui`：通过，`problems: []`。
- `pnpm check:browse`：通过；用 CDP 真鼠标事件将把手向上拖 18px，确认第 4 档变为第 6 档，
  再向下拖 18px 回到第 4 档；随后原有滚轮缩放、2 秒防抖写入与四态循环继续通过。
- Pencil：普通 film 与 compare film 的内存画布均增加同样的 8px 三点条；截图检查未发现新增坍塌，
  报告的胶片带横向裁切与长信息栏裁切属于画布原有的滚动 / 示例内容表现。本地 `.pen` 时间戳
  未变化，Computer Use 又因当前 WSL 工作区 URI 无法初始化，因此没有冒充已保存落盘。

## 未由 Agent 声称验证的 E2E

- Windows WebView2 真机下拖动阻尼、吸附手感和在不同 DPI 下的三点可见度。
- 鼠标连续快速来回拖动后，真实 `app.db` 是否在停止约 2 秒后只留下最终档位。
- 在 Pencil 中按一次 `Ctrl+S`，再由 Agent 核对 `design/browse.pen` 已进入 Git 工作区差异。
