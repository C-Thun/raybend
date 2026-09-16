# 弹窗骨架、开关可见性、目录树「展开即重读」、左列宽度把手

完成时间：2026-09-16 15:34:36 CST

## 一、目录树：**展开即重读那一级**（撤掉「刷新」按钮）

人类的原则（原话）：*「也不能加刷新啊，哪家文件管理器要靠刷新才能看到实际情况的？
目录缓存可以加，加的目的是展开时流畅……但必须每次展开刷新。这个成本高吗？
又不是递归下级全扫，就一级啊。」*

改法：

* `features/dir-tree/store.ts` 的 `expand`：**去掉「读过就不再读」的短路** ——
  每次展开都重读那一级的直接子目录（一级 `readdir`，非递归）；
* 缓存只承担「展开的瞬间就有内容可看」：`load` 只在**成功时**覆盖，
  所以重读期间旧内容留着、失败也不清空（不会表现成「这个目录是空的」）；
* **撤掉「来源」面板的刷新按钮**（`common.refresh` 键一并删除）；
* 替代品是文件管理器的常规行为：**窗口重新获得焦点时**重读展开着的目录（`DirTree` 内）
  + 重列卷（`LeftColumn` 内）；
* 演示加「模拟外部新增目录」按钮 → 冒烟断言「外部新增的目录，**展开就能看到**」。

## 二、开关在卡片上不可见（截图报回来的）

对照设计稿（`design/main.pen`），两处都是实现没跟上设计：

| 项 | 设计稿 | 实现（错） | 结果 |
| --- | --- | --- | --- |
| 关闭态轨道 | `Switch / … / off` = **`$surface-main`** | `bg-surface-track` | 卡片本身就是 `surface-track`（`SelectedBar` 也是）→ 完全同色，整个开关只剩一个灰点 |
| 「包含子目录」标签 | `SubdirLabel` = `$fg-3` / 13px（**可见文字**） | 只给了 `aria-label` | 屏幕上根本没有文字 |

* `Form.tsx`：关闭态轨道改 `bg-surface-main`，并写明用法约束（开关放在比 main 亮的面上）；
* `SelectedDirs.tsx`：补上可见标签（`aria-hidden`，读屏走 Switch 的无障碍名）；
* 画廊加「已选目录条」同构样例，冒烟**实测颜色**：轨道/标签与卡片底色的 RGB 距离必须 > 18
  （钉死「同色不可见」这类回归 —— 单测抓不到它，颜色是令牌算出来的）。

## 三、弹窗骨架（截图报回来的）

设计稿 `Dialog / 新建库` 的数值：**padding 16 / gap 12 / 标题 17px semibold / 正文 13px 行高 1.5 /
底部按钮高 32**。实现当时是：内边距跟密度缩到 **6px**、标题 **13px**、正文 12px、底部按钮靠 `py` 撑。

* `tokens.css`：新增 **`--dialog-pad: 16px`**（两档密度同值 —— 弹窗不吃密度档）；
* `Dialog.tsx`：padding 16 / gap 12、标题 17px `leading-6`（**行盒与右上角的叉等高 → 中心对齐**）、
  正文 13px、底部按钮统一 `h-8`（后代选择器，免得每个调用点各写一遍）；
* `Button.tsx`：**高度显式写死 + `leading-none`**（人类反馈「按钮里的文字明显上移」）——
  行高继承 1.5 时 flex 居中的是**行盒**，拉丁字体的 ascent/descent 不对称，文字就看着偏上；
  钉住行高后居中在字形盒本身，不随字重漂移；
* 冒烟新增 `dialogFrame` 断言：实测 `padTop 16 / padLeft 16 / gap 12 / 标题 17px / 标题与叉中心重合`。

## 四、左列宽度把手（去掉嵌套 Ark Splitter）

自写把手（12%–50% + 220px 下限 + 方向键），容器里只剩左列内部那一个竖向 splitter。
详见 `2026-09-16_left-column-handle-and-stale-win-build.md`。

## 五、涉及文件

* `src/styles/tokens.css`、`src/components/ui/{Dialog,Button,Form}.tsx`
* `src/features/selected-dirs/SelectedDirs.tsx`、`src/dev/{KitchenSink,dir-tree-demo}.tsx`
* `src/features/dir-tree/{store.ts,DirTree.tsx}`、`src/workspaces/import/LeftColumn.tsx`
* `src/i18n/{zh-CN,en-US}.ts`（删 `common.refresh`）
* `scripts/ui-smoke.mjs`（`switchBar` + `dialogFrame` 断言；删掉过时的刷新按钮断言）
* 测试：`src/features/dir-tree/store.test.ts` 三条

## 六、验证（Agent 侧）

* `pnpm typecheck` / `pnpm test`（**401 通过**）/ `pnpm lint:arch` / `pnpm build` 全绿
* `pnpm smoke:ui` → `problems: []`（`dialogFrame`、`switchBar`、`widthHandle`、`leftColumn`、
  `dirTree` 含外部新增可见、`resizeProbe` 全符合预期）

## 七、遗留

* **真机视觉仍归人类**（弹窗骨架、开关对比度、按钮文字居中只能在真机目视确认）。
* 字体：更纱黑体接入方式待定 —— 全量单文件约 10–18MB/字重且**不分片**，会让启动变慢；
  现在的 Inter + Noto Sans SC 是按 unicode-range 分片的，启动很轻。
