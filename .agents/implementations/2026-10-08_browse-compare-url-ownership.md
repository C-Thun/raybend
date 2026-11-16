完成时间：2026-10-08 23:07:41 +0800

# 浏览对比「点一张、另一张空白」——URL 所有权修复 + 取图探针

## 起因（崔总报的症状）

在 browse 里做 2 图对比，**多次缩放/点画幅之后**，左右两格同一时间只有一格有图：
点到哪张哪张显示，另一张变空白。程序运行着，界面上没有报错。

## 根因（已用单测钉住）

`components/ui/viewer/store.ts` 里一个 URL 会被三处用着：单图槽位（`currentUrl`）、总览槽位
（`overview()`）、多图缓存（`imageUrls()`）。旧实现是「**取出即归调用方所有**」：

* `takeCachedImageUrl()` 把当前那张的 URL 从多图缓存里**摘走**（delete 掉条目），交给单图槽位；
* 单图槽位换图/关闭时按 `revokeUrl()` 回收它。

于是焦点在画幅之间来回切时：

1. 点 B → B 的缓存条目被摘走，B 的 URL 住进单图槽位（这一步 B 还显示得出来，靠 `imageUrlFor` 的兜底）；
2. 再点 A → 单图槽位换成 A，**顺手把 B 那份 URL 回收**；而 B 的缓存条目在上一步已经没了 ⇒
   `imageUrlFor(B)` 返回 `null` ⇒ 那一格的 `<img>` 被 `<Show>` 卸载 ⇒ **整块空白**。
3. 再点 B 会重新发一次取图请求（所以「点到哪张哪张就显示」）。

复现测试：`store.test.ts` 的「对比：来回点画幅不能把另一幅的 URL 弄丢」——修前红、修后绿。
触发条件与「缩放」无关，要点在于**点画幅会切当前照片**（`pointerdown` 就切，见 `CompareView` 的
`focusFromTarget`），拖拽/点选时一直在发生。

## 修法：持有者检查（谁都不用了才回收）

* `takeCachedImageUrl` → `borrowCachedImageUrl`：**借用不摘走**，缓存继续持有（对比的其它画幅还要它）。
* 新增 `holderOf(url)` / `releaseUrl(url, reason)`：任何一处放手都先问「单图槽位 / 总览槽位 /
  多图缓存还有谁在引用」，有 ⇒ 只记一笔探针事件、**不 revoke**；没有 ⇒ 这才 revoke。
* 从未发布出去的 URL（重复结果、迟到结果、解码失败）走 `dropUrl(url, reason)` 直接回收。
* LRU 淘汰与 `clearImageUrls` 改成**先提交新表、再放手**：`releaseUrl` 读的是已提交的
  `imageUrls()`，在 setter 里提前 revoke 会看到旧表（还持有自己）而漏回收。

不变式（测试里各有一条）：不双回收（同一 URL 只 revoke 一次）、不泄漏（`close()` 后每个 URL
都被回收，探针账本归零）、不重复解码（借用仍免掉第二次 IPC，原有测试继续绿）。

## 探针（崔总问的「要不要准备支持探针的 debug 模式」）

新增 `components/ui/viewer/probe.ts`：把 URL 的一生（ensure / new / borrow / release / revoke /
evict / invalidate / **missing**）记成环形事件流（160 条），并维护「每张照片还活着几个 URL」。

* 开关：`localStorage["raybend.viewer-probe.v1"]="1"`（打包版也能用）／ `Ctrl+Alt+Shift+D`（DEV 热键）／
  控制台 `__raybendProbe.dump() / .events() / .enable()`；**关着时 `record()` 第一件事就是返回**，
  生产构建里没有监听器、没有定时器。
* 判据分裂点：空白而 `live = 0` ⇒ URL 被提前回收（存储层问题）；空白而 `live > 0` ⇒ 视图/布局没用上。
  这一次的 bug 正是**前者**，`missing` 事件一记就能看见。
* 视图侧：对比每一格永远带 `data-compare-url="ok|missing"`（devtools/脚本可读），探针开着时每格还有
  一行读数、面板上挂最近 6 条事件。

配套的**回归靶子**（真 DOM）：

* `src/dev/compare-demo.tsx` + `src/dev/demo-photos.ts`：陈列室里的对比演示（真 `CompareView` + 假后端，
  三张尺寸不同的假图），`pnpm smoke:ui` 按「滚轮缩放三次 → 逐格点三轮」跑，每次点击后**每一格都必须
  还有图**，并顺带验探针热键能开能关。
* `viewer-demo.tsx` 的假图抽到 `demo-photos.ts`（同一个能力只留一份）。

## 涉及文件

* `src/components/ui/viewer/store.ts`、`store.test.ts`（+3 条回归：对比往返 / 淘汰推迟回收 / 探针账本）
* `src/components/ui/viewer/probe.ts`（新）、`probe.test.ts`（新，6 条）
* `src/components/ui/viewer/CompareView.tsx`（`data-compare-url` + 探针浮层）
* `src/dev/compare-demo.tsx`（新）、`src/dev/demo-photos.ts`（新）、`src/dev/viewer-demo.tsx`、`src/dev/KitchenSink.tsx`
* `scripts/ui-smoke.mjs`（对比段 + 探针段）

## 验证

* `node --test src/components/ui/viewer/store.test.ts`：36 条全绿（含新回归；修前「切回 a 之后 b 仍应可显示」红）
* `pnpm test`：1225 条全绿 ｜ `pnpm typecheck` / `lint:colors` / `lint:arch` / `lint:i18n` / `pnpm build` 全绿
* `pnpm smoke:ui`（临时起的 `pnpm dev`）：`problems: []` —— 含新的对比断言与探针开关断言
* 未验证（需崔总真机）：真实照片库下的多图对比手感、探针浮层在 Windows WebView2 里的观感
