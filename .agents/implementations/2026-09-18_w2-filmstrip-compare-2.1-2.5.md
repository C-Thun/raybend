# W2 阶段 2：胶片带与对比（2.1–2.5）

完成时间：2026-09-18 23:58:41 CST

范围：`plans/M2-W2.md` 阶段 2 的五步 —— 胶片带、多选进对比、对比画幅与同步、对比态的特殊胶片带、点哪张=当前那张。

---

## 一、2.1 胶片带（`src/features/browse/FilmStrip.tsx`）

画布取值（`Shell / Browse / View` 的 `FilmStrip` 节点）：**104 高**、缩略 **96×80**、
间距 6、左右内边距 8、底 `$surface-main`、缩略圆角 4；选中 `$state-selected`、
**当前那张**再加 1px `$brand` 描边。

三条纪律（都落到了代码与测试里）：

1. **选择逻辑与 tiles 共用**，不是「照着写一遍」：
   * 新抽 `lib/selection.ts` 的 **`clickMode(event)`**（修饰键 → 模式，Shift 优先于 Ctrl），
     网格与胶片带现在都调它 —— 以前网格里那段三元表达式已删掉；
   * 区间/翻转语义仍走同一个 `store.select()`。
2. **列表就是看图件手里那一份**（`viewer.state().photos`）——胶片带与看图永远同源。
3. **缩略图队列网格与胶片带共用一个**：队列提到工作区创建，`BrowseGrid` 新增可选
   `thumbs` 属性（不传就自己建，保持组件独立）。网格里已缓存的照片胶片带立刻就有。
4. 锁徽标（`BROWSE.md` §3.1：tiles / 看图 / 胶片带都要看得出来）以 10px 图标压在右上角。

**中列结构调整**：看图盖层原来盖整个中列，胶片带要留在它下面 ⇒ 改成
「照片区（`relative flex flex-col`：网格 + 看图盖层）」+「控制条（tiles 时）」+「胶片带（看图时）」。
⚠️ 那个 `flex flex-col` 不能省：网格根节点靠 `flex-1` 撑高，外面换成普通块级盒子它会塌成 0 高
（虚拟列表一屏都不渲染 —— 冒烟里当场表现为 `tiles: 0`）。

## 二、2.2 多选 → 自动进对比（`src/features/browse/compare.ts`）

**没有 `enterCompare()` 这种动作**：对比态是**选择状态的派生值**。

```text
看图 + 选中 ≥ 2 张  ⇒ 对比
反选到只剩 1 张     ⇒ 自动回单张看图
```

`compareIds(选中集合, 显示顺序)` 给出「该对比哪几张」——顺序按**显示顺序**（不是点击顺序），
因为胶片带也只显示对比图、顺序必须一致。

## 三、2.3 画幅与同步（同文件 + `CompareView.tsx`）

人类的两条硬要求都由纯函数承担（8 条测试盯着）：

* **以第一幅的比例为准扣等比例区域**：`cropToAspect()` —— 比基准宽就扣两侧、比基准高就扣上下、
  比例一致时**一个像素都不扣**（避免因浮点误差产生 0.5px 的滑动）；
* **位移按百分比同步**：`panPercent()` / `percentToPan()` —— 「挪了画幅的 5%」而不是「各挪 200px」。

渲染**不碰像素**（红线）：图片按「原图 / 扣取区」放大、再按扣取起点平移，扣出来的那块正好铺满画框 ——
全是几何变换。画框本身必须**按基准比例定形**（`aspect-ratio` + 高撑满 + `max-width` 兜底）：
少了这一层，扣出来的 4:3 会被拉成「剩余宽 × 整个高」，两边同时变形
（写这个组件时先漏了这一层，review 里补上）。

第一幅带 1px 主色内描边（它是基准）。

## 四、2.4 对比态的特殊胶片带

对比中再按一次**回车** → 胶片带只显示参与对比的图，**整条加 1px 主色细边框**
（画布 `Shell / Browse / Compare` 的 `FilmStrip` 节点就是带 stroke 的那条）；
此状态下**不按 Ctrl**点一张也是 Ctrl 的效果（点什么就把它移出对比）。

退出条件写成了**派生逻辑**（`createEffect`：不再处于对比态就复位），而不是各处手动清 ——
「只剩一幅就退出」是选择状态决定的事，手动清一定会漏。

**冒烟抓到的语义 bug**：在「只看对比图」里点掉一张时，画面原本会跳到**被移出的**那张上
（右栏与状态栏随之错位）。正确行为是落到**还在对比里的**那张（`BROWSE.md` §5.5 的
「定位到这最后一张图的位置」）。修法：`goToSurvivor()`；并且要**先捕获**对比集合再动选择 ——
`select()` 一执行，选择只剩一张、对比态立刻退出、`onlyIds` 变回 `undefined`，
那时再读就已经晚了（这个坑当场被冒烟的第二条断言抓住）。

## 五、2.5 点哪张图 = 当前那张

* `BrowseStore` 新增 **`setAnchor(id)`**：**只挪锚点、不动选择集合**。
  对比里点某一幅画幅就是这件事 —— 走 `select()` 会把选择改成只剩这一张，对比当场散掉。
  纪律：锚点必须在选择集合里，不在就不动（3 条单测）。
* `CompareView` 的点按回调把「当前那张」交给外面：看图件切过去（状态栏/胶片带跟着）
  + 把锚点挪过去（右栏跟着）。
* 冒烟断言同时钉住了反面：点之前右栏显示的是另一张（否则这条断言区分不出「跟着走」）。

## 六、验证

```text
cargo test -p raybend                    791 passed / 1 ignored
npx tsc --noEmit                         0 error
pnpm test                                571 passed（新增 compare 18 条 + clickMode 3 条 + setAnchor 3 条）
pnpm lint:arch / lint:colors / lint:i18n 通过
pnpm build                               通过
pnpm smoke:ui                            problems: []
pnpm check:browse                        通过（新增：胶片带 3 张/切图、Tab 三态带胶片带、
                                         Ctrl 多选进对比、对比画幅与基准、2.4 的两种退出路径、2.5 跟随）
```

> **未经人类验证**（`AGENTS.md` §2.8）：胶片带的滚动手感与视觉密度、对比画幅的观感
> （扣取是否真的「居中得体」、1px 描边是否够明显）、对比态下滚轮缩放/拖动的体感 ——
> 冒烟只证明结构与交互接线正确，好不好看、顺不顺手得人类在真机上看。

## 七、一处需要人类确认的口径

选中 **超过 4 张**时（`COMPARE_MAX = 4`，来自「对比 2–4 张」）：现在的做法是
**取显示顺序的前 4 张**，并在画面底部留一句「最多同时对比 4 张，这里显示前 4 张」。
不静默截断是刻意的选择，但「取前 4 张」本身仍是个判断 —— 若您想要的是别的行为
（例如超过 4 张干脆不进对比、或按最近点的 4 张），改一处常量 + 一次过滤即可。

## 八、涉及文件

```text
新增  src/features/browse/FilmStrip.tsx / CompareView.tsx / compare.ts / compare.test.ts
改动  src/lib/selection.ts（+clickMode）/ selection.test.ts（+3）
改动  src/features/browse/store.ts（+setAnchor）/ store.test.ts（+3）
改动  src/features/browse/BrowseGrid.tsx（+thumbs 可选属性；改用 clickMode）
改动  src/features/browse/index.ts（导出）
改动  src/components/ui/viewer/store.ts（+goTo 公开）/ index.ts
改动  src/workspaces/browse/BrowseWorkspace.tsx（照片区结构、共享队列、对比态派生、Enter 语义）
改动  src/i18n/{zh-CN,en-US}.ts（compareNoSize / compareLimit）
改动  scripts/check-browse-boot.mjs（胶片带 / 对比 / 2.4 / 2.5 共 4 组断言）
```
