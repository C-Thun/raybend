# 筛选态下赞/踩（与星级阈值）的按下态

完成时间：2026-09-20 04:15:45 CST

## 范围

人类 2026-09-20 报的 bug：**筛选状态下，赞和踩被选中时不显示状态**。

改动两处：

* `src/features/browse/BrowseToolbar.tsx` —— 修显示口径；
* `scripts/check-browse-boot.mjs` —— 补三条回归断言（筛选段内）。

## 根因

工具条那几个控件的按下态**以前只读「选中照片的三态」**（`ratingState()` / `likeState()`），
而筛选条件一改，`store.patchFilter()` → `reload()` → `resetForNewQuery()` 会把选中清空，
于是 `triState([])` 恒为「无值」⇒ 永远显示没按下。

同一个文件里**色标与锁早就是「筛选态读 `store.filter()`」的口径**
（`colorState()` 旁边的 `filtered()`、锁的 `isSet() || filtered()`），
星级与赞/踩是漏掉的两处 —— 属于 `AGENTS.md` §2.12「同一个东西两种表达」的同类问题，
所以一并修（星标阈值在筛选态下同样是「点了筛了但控件不亮」）。

## 关键决策

* **赞/踩的按下判据用 `.includes()` 而不是「第一个值等于它」**：
  `filterFromSelection()` 可能一次给出两个值（例：选中的照片里既有「喜欢」也有「未标记」⇒ `likes = ["like", "none"]`），
  用 `.includes()` 才与**色标 / 锁**同一口径（组内是「或」，两个条件都该亮）。
  工具条自己设值时仍是互斥的（`markLike` 写 `[value]` 或 `[]`），两者不冲突。
* **星标阈值点亮 1..N 颗**（不是只亮第 N 颗）：与标记态「3 星亮 3 颗」同一套视觉，
  语义由 chips 那行 `≥N 星` 承担（`BROWSE.md` §3.1 已定阈值语义）。
* 新增两个具名派生函数 `likePressed(value)` / `starFilled(star)`，把
  「标记态读选中、筛选态读条件」这条口径收在**一处**（原来它散在渲染表达式里）。

## 验证

* **回归断言先红后绿**（不然就是假绿）：
  * 把 `BrowseToolbar.tsx` 暂存回旧版（`git stash push -- <file>`）跑 `pnpm check:browse`：
    ✗ 三条全中 ——
    「筛选条件里有『喜欢』时，喜欢按钮必须是按下态（实测 like=false）」
    「筛选态点『不喜欢』应当把条件换成它（实测 like=false, dislike=false）」
    「筛选态星标是阈值（实测 s1..s5 全 false）」；
  * 恢复修复后再跑：✓。
* 断言里**先按 `Esc` 清掉选中**再读按下态 —— 不清的话旧代码可能碰巧读到一张
  恰好是「喜欢」的照片（上一步刚点过），这条断言就假绿了（第一次跑红时确实如此，已补）。
* 门禁：`pnpm typecheck` 0 ｜ `pnpm test` 709 ｜ `pnpm lint:colors` / `lint:arch` / `lint:i18n` ✓ ｜
  `pnpm build` ✓ ｜ `timeout 400 pnpm check:browse` ✓。
* Rust 侧未动，不跑 cargo。

## 遗留

* 真机目视仍归人类（`AGENTS.md` §2.8）：筛选态下赞/踩/星标的点亮观感。
* `design/browse.pen` 未同步（本轮只改行为，画布上筛选态画的就是「按下」形态，无需改形状）。
