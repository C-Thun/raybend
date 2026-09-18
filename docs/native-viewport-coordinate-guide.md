# 原生 GPU 视口开发指导：从 Windows 文字缩放错位中建立可靠坐标契约

本报告对应 2026-09-19 的 spike 修复。对象是 `src/dev/SpikeViewport.tsx`、
`src-tauri/src/spike_viewport.rs` 与 `crates/raybend/src/render/`，不是 M2 的 DOM 看图组件。

## 1. 结论与证据等级

**本次主要错误是把 Windows 显示缩放当成 WebView 的有效 CSS 像素比例。**
系统显示缩放为 125%，Windows 辅助功能中的文字大小为 110%，WebView2 内容的有效比例为
`1.25 × 1.10 = 1.375`。旧代码只取 Tauri `scale_factor() = 1.25`。
因此洞口、命中测试、拖动和缩放锚点共同用了错误的 CSS → 物理像素比例。

证据不是“感觉差一个标题栏”，而是下面三个彼此独立的来源：

1. 崔总提供的 `QQ_1789714051780.png`：DOM 洞口宽约 720px，而左栏声称物理宽为 655px。
2. 本机只读查询 `HKCU\Software\Microsoft\Accessibility\TextScaleFactor`：值为 `0x6e = 110`。
3. 微软的 [WebView2 RasterizationScale 文档](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2controller.rasterizationscale)
   明确说明其比例结合显示器 DPI 与系统文字缩放。页面缩放还可能进一步改变有效比例，
   因而运行时应读 `window.devicePixelRatio`，不能写死乘 1.1。

**验证边界**：代码回归测试与离屏 GPU 像素回读已验证换算及着色器输出；
Windows 桌面合成、真实窗口中的目视对齐与跨屏体感仍需要真机验收。
不能把离屏通过写成“Windows GUI 已通过”。

## 2. 截图如何直接证明问题

截图读数：客户区 `1598×1022`，洞口 CSS `(300,37,524,707)`，
旧 DPR `1.25`，缩放 `1.0523`，pan `(189.2,-19.4)`，旋转 `0°`。

以下坐标相对客户区；截图边框/标题栏让全图坐标另外多约 `(1,38)`。
截图读出的边界只有约 1–2px 精度，不能拿它判断亚像素误差。

| 项目 | 旧算法（×1.25） | 实际 DOM（×1.375） |
| --- | ---: | ---: |
| 洞口左边 | 375 | 412.5 |
| 洞口上边 | 46.25 | 50.875 |
| 洞口宽 | 655 | 720.5 |
| 洞口高 | 883.75 | 972.125 |
| 洞口右边 | 1030 | 1133 |
| 洞口下边 | 930 | 1023（再裁到客户区） |

灰块右边确实停在截图约 x=1030，右面板约从 x=1134 开始，正好漏出约 103px 的桌面。
底部灰块到全图约 y=968 停止，也符合 `38 + 930`。

旧算法预测图心：

```text
客户区内图心 = 洞口中心 + pan
             = (375+655/2, 46.25+883.75/2) + (189.2,-19.4)
             = (891.7,468.725)
截图图心约   = (892.7,506.725)
```

它与图中紫色十字约 `(893,507)` 一致。说明 GPU 正在忠实执行错误比例算出的布局。
**灰块和十字不是两个独立 bug，也没有证据支持给输入统一加减一个标题栏高度。**
比例错误产生的偏差会随位置而增大；固定平移只能碰巧修好一个点。

## 3. 这个测试窗口本来应该是什么样

窗口由左右诊断面板、中间透明洞口组成。GPU 在透明洞口后面绘制一张 6000×4000 的
灰色渐变合成测试图。紫色十字与中央白块属于这张图，四角的红/绿/蓝/黄标记用来认方向。
照片本身不透明；**只有洞口内没有被照片覆盖的部分才露出桌面**。

| 操作 | 正常外观 | 异常判据 |
| --- | --- | --- |
| 点“复位”（Fit，pan=0，旋转=0） | 整张 3:2 图居中于洞口；较长方向两侧对称留白；四角标记可见 | 单侧宽条、十字不居中、明显偏向一边 |
| 接着点“1:1” | 图像巨大，只看到图心附近；本机洞口应被灰图填满，十字位于洞口中心 | 右侧或底部仍漏出宽条 |
| 鼠标放在十字心再滚轮 | 十字心留在光标下面；臂长随缩放变化 | 十字绕远处某点移动 |
| 主动拖动或绕非中心点缩放 | pan 改变，十字可以离开洞口中心 | 不能再用“十字必须居中”判错，应先复位 |
| 关闭“洞口”裁剪 | GPU 以整窗为参照；左右 DOM 面板仍遮住下层 | 此状态不用于判定洞口居中 |

不要在 1:1 下寻找图像四角；不要把灰色照片误认为“透明失败”；
不要把 Fit 的对称留白误认为明条；不要先做 25 分钟全套测试才发现第一步已经错位。

## 4. 必须长期遵守的坐标契约

### 四种量不能混用

| 名称 | 来源 | 单位与用途 |
| --- | --- | --- |
| native scale | Tauri `scale_factor()` / 显示器 DPI | 原生窗口逻辑尺寸转换、环境诊断 |
| WebView DPR | `window.devicePixelRatio` | DOM client/CSS 坐标 → surface 物理像素 |
| DOM viewport | `window.innerWidth/innerHeight` | WebView 实际 CSS 视口；不能拿 native size / native scale 冒充 |
| image zoom | Rust `Viewport.zoom` | 图像像素 → 物理像素；1:1 永远是 1.0 |

前端只报告原始事实：`getBoundingClientRect()`、`clientX/Y`、拖动 CSS 位移、DPR。
**所有物理坐标换算仍然在 Rust**。上报 DPR 不等于把变换数学搬到前端。

```text
pointer_physical = pointer_client_css × webview_dpr
hole_physical    = hole_client_css × webview_dpr
pan_delta        = pointer_delta_css × webview_dpr
image_point      = inverse_viewport_transform(pointer_physical)
```

这些公式的前提是单个全窗 WebView 的客户区原点与 surface 客户区原点重合。
以后若引入偏移子 WebView，必须增加**测得的原生容器偏移**，并同时作用于输入和洞口。
不要用 `window.screenX/Y × dpr` 猜它：屏幕坐标、窗口外框、客户区以及混合 DPI 桌面不是同一坐标域。

### 尺寸/DPR 变化必须收口处理

保存 CSS 洞口为布局真相，每次 DPR 变化重新生成物理洞口。不能反复缩放上次整数化的矩形，
也不能让 Windows Resize 消息把已测得的 WebView DPR 改回 OS scale。

本次 `HoleRect` 与指针、滚轮、拖动消息均携带取样时的 DPR；布局还报告真实 DOM 视口尺寸。
`ResizeObserver` 负责尺寸变化，低频布局比对补上 DPR 变化及只有位置变化的情况。
保留最后 CSS 洞口时，要把“关闭裁剪”作为独立状态，不能被下一次布局上报偷偷打开。

目前 spike 的原生窗口事件与 DOM 事件仍是独立队列，跨屏过渡期间可能先后到达。
最终编辑模块应使用带递增 geometry revision 的整包布局事务，使旧输入无法套用新布局。
本修复解决稳定状态比例错误，不声称已经保证跨进程每一过渡帧完全同步。

## 5. 诊断工具为什么曾经给出错误信心

| 旧检查 | 实际只能证明什么 | 改进 |
| --- | --- | --- |
| “物理洞口 = CSS 洞口 × 左栏 DPR” | 程序使用了它自己的 DPR，不能证明 DPR 正确 | 同时记录实际 DOM 视口、WebView DPR、native scale |
| “CSS = 客户区尺寸 / native scale” | 只是计算结果，不是浏览器测量 | 改用 `innerWidth/innerHeight` |
| “往返误差 = 0” | 正逆变换互逆；共同错误的单位也能得到 0 | 单独标为数学自查，与像素回读及真机对照分开 |
| “命中中心 ✅” | 上次采样在当前模型里被判为中心 | 保存并显示/报告采样 CSS 与预测图心；不得声称坐标同步已经正确 |
| “输入偏移 = screenX × DPR − clientOrigin” | 混用了不同坐标域；且旧 IPC 字段命名不匹配 | 从主线程原生 WebView bounds 获取容器位置，失败就未知 |
| “鼠标每 60ms 报一次” | 节流窗口内的最后事件可能被永久丢掉 | 合并时保留尾样本，停手也必须发送；滚轮前立即刷新 |
| “睡 8ms 后读状态” | 没有任何完成保证 | 本次仍保留旧异步返回模式；后续产品必须用命令序号/确认或帧快照 |

旧调查报告中“单位无误”“就是原点问题”的断言均被此次证据推翻。
报告的价值在于保留原始现象，不应把未经独立测量的推断升格成排除条件。

## 6. 开发时如何验证，才不反复消耗真机测试

按从便宜到昂贵的顺序：

1. **纯状态单测**：把 OS 1.25、WebView 1.375 当作不同输入；验证洞口、缩放锚点、DPI 切换、
   零洞口、关闭/重开裁剪、非法比例和静止指针重算。不要只覆盖两者相等的 1.0/1.25/1.5。
2. **IPC 单测**：使用真实前端字段名反序列化；枚举 `rename_all` 只管变体名，字段需要
   `rename_all_fields`。缺失 DPR 必须报错，不能悄悄回退到错误坐标系。
3. **离屏 GPU 冒烟**：复用生产 shader/矩阵/管线，回读固定物理像素。此轮新增回归断言：
   `(1120,500)` 必须有照片、`(1140,500)` 必须透明、`(773,537)` 的白块在缩放前后保持。
   判据应包含外部给定的像素位置，避免被测函数自己生成所有期望值。
4. **构建核对**：前端 `build` 在 Windows 编译之前；`check:win` 同时核对时间和资源名。
   运行中的 exe 会阻止覆盖，先正常退出应用。debug 构建不是发布；不自动 push/tag/release。
5. **最短真机门槛**：只做“复位 → 1:1 → 十字缩放”。这三步不通过就停止全套性能/DPI表，
   写诊断快照。通过后再测跨屏、最小化、透明合成、设备恢复。

常用命令（在仓库根）：

```bash
pnpm typecheck
pnpm test
pnpm lint:colors
pnpm lint:arch
pnpm lint:i18n
cargo test -p raybend-desktop spike_viewport::tests --lib
cargo run -p raybend --example spike-offscreen -- /tmp/raybend-spike-coordinate-fix
pnpm spike:win --dry-run
pnpm spike:win
```

“写报告”现在会额外生成 `spike-coordinates.json`，保存当前洞口、真实 CSS 尺寸、WebView DPR、
系统 scale、原生容器原点、最后指针 CSS 与预测图心。它与 `spike-report.md/json` 一起交接。
不再要求崔总手抄所有坐标。

## 7. 后续编辑模块的约束与未完成项

- 所有覆盖层使用 Rust 的同一份变换；若前端需要画 CSS 覆盖层，由 Rust 同时返回相应 CSS 坐标。
- 不通过强制文字缩放 100%、禁用辅助功能或硬编码 1.1 来掩盖坐标契约问题。
- 原生窗口查询只在主线程获取，再推纯数据到渲染线程；不能在渲染线程持共享锁查询窗口。
- DPR 来源不同于 native scale，需要在架构文档、DTO 和字段注释中明确写出。
- 帧确认/几何 revision、完整设备恢复、空闲无重绘、GPU 挂起时关闭窗口的生命周期，
  属于产品化前需要另外完成的工作。此次没有把整套 spike 宣称为可直接复用的编辑模块。
- 手工填写过的验收项保留历史，不因代码编译通过就自动勾选新版验收。

参考资料：

- [WebView2 RasterizationScale：DPI 与文字缩放](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2controller.rasterizationscale)
- [devicePixelRatio 的定义与页面缩放](https://developer.mozilla.org/en-US/docs/Web/API/Window/devicePixelRatio)
- [DirectComposition 基本坐标：根 visual 相对客户区](https://learn.microsoft.com/en-us/windows/win32/directcomp/basic-concepts)
- 本仓 `implementations/2026-09-18_spike-coordinate-offset-report.md`（旧推断，已补勘误）
