# 浏览左列：点库不铺图 / 树根是 photos/ / 隐藏 _RAW / 库列表紧缩展开

完成时间：2026-09-18 16:23:48 CST

计划：`plans/M2-W2.md` 阶段 0.2（三项修正）与阶段 0.8（库列表口径）
依据：人类 2026-09-18 两次口述 —— 第一次报三处实现错误，第二次逐条重述库列表规则。
相关：`BROWSE.md` §4.2 / §4.3、`design/browse.md` §5.00–§5.01

---

## 一、三处修正（阶段 0.2）

| # | 现象 | 根因 | 改法 |
| --- | --- | --- | --- |
| 1 | 点一下库就把整库照片铺出来 | `store.setRepository()` 把 scope 置 null，而后端把 `scopePath: null` 当「整个库」 | `query()` 在**没选目录时不构造查询**；网格显示「挑一个目录」的空态；删掉左列的「整个库」入口 |
| 2 | 目录树上多一层 `photos` | 树从**库根**读起 | 树根改为 `photos/` 之内（`PHOTOS_DIR`）；**查询参数不动**（`asset_files.rel_path` 仍是 `photos/…` 口径） |
| 3 | 保留目录 `_RAW` 显示在树上 | 没过滤 | 任何层级都滤掉 `_RAW`（`visibleChildDirs()`）；RAW 在网格里怎么呈现**待人类后续说明** |

抽成纯函数放在 `src/features/browse/dirs.ts`（`isRawDirName` / `visibleChildDirs` / `dirDisplayName`），
带 8 条单测（大小写、空白、全角、结尾斜杠、反斜杠、Unicode、超长名、空输入）。

## 二、库列表紧缩 / 展开（阶段 0.8）

规则（`BROWSE.md` §4.2 重写版）：≤3 个库 = **只有缩起态**、不显示「查看所有库」、不留 3.5 张卡的空高；
>3 个库 = 前 3 张 + 第 4 位**伪卡片**（同宽同高同圆角、只有一行「查看所有库」靠卡片顶部），
容器高度钉成 3.5 张卡、`overflow: hidden` 切掉下半截；展开要**先读齐数据再换界面**，
下面的目录树**缩到最小值**（不是藏掉）；移顶**只在展开态**；
收起只由三件事触发：**15 秒没再点库 / 在 browse mid 点了一下 / 切 flow**。

| 文件 | 改动 |
| --- | --- |
| `src/features/browse/libs.ts`（新） | `compactList()` / `listHeightStyle()` / `COMPACT_REPO_LIMIT` / `TREE_MIN_HEIGHT_PX` / `EXPANDED_IDLE_MS`，带 8 条单测 |
| `src/features/browse/BrowsePanels.tsx` | 库列表用 `compactList` 渲染 + 伪卡片；展开态列表可滚、目录树缩到 `TREE_MIN_HEIGHT_PX`；15 秒定时器（每次点库顺延） |
| `src/features/browse/BrowseGrid.tsx` | 新 prop `onInteract`（点中某张照片时回调） |
| `src/workspaces/browse/BrowseWorkspace.tsx` | 持有 `libsExpanded` 状态；网格点击 → 收起；切工作流时工作区卸载 → 自然回到缩起态 |

**状态为什么住在工作区**：收起要由**网格侧的点击**触发，而那个点击发生在左列之外；
住在左列里就没法从外面收。

## 三、顺带抓到的一个 Solid 响应式坑（值得记）

冒烟第一次跑红：中间列一直显示「还没有库」，而控制条已经显示库名 —— 同一个 store，两处不一致。

根因：`<Show when={watermark()}>` 里 `watermark()` 返回的是**水印元素本身**，
而 Solid 的 `Show` 在非 `keyed` 模式下比的是**真值**（`!a === !b`）——
「还没有库」与「挑一个目录」都是真值，于是它认为「没变」、不重渲染，界面永远停在第一个水印上。

修法：`<Show when={watermark()} keyed>`（`keyed` 按对象比较），并留注释说明判据。
同类先例见 `components/ui/EasyCopy.tsx` 的 `popKey`。
**注意**：`<Show when={error()}>` 这类「字符串条件」也有同样的隐患（非空串换成另一个非空串不会更新），
本轮没动它们（没有实际症状），记在这里备查。

## 四、验证

```text
npx tsc --noEmit                    0 error
pnpm test                          525 passed（新增 dirs 8 条 + libs 8 条 + store 1 条）
pnpm lint:colors / lint:arch / lint:i18n   通过
pnpm build                         通过
pnpm check:browse                  通过（fixture 改成 **5 个库**，新增三条断言：
                                   树首层无 photos / 无 _RAW / 未选目录时网格是空态；
                                   再加：5 个库时出现「查看所有库」、点开后全部库可见、目录树仍在）
pnpm smoke:ui                      problems: []
```

## 五、遗留

* `_RAW` 里的 RAW 照片**在网格里怎么呈现**（现在按独立资产参与查询，与同名位图同格子还是各占一格）
  等人类后续说明；本轮只做了「树里不显示这个目录」。
* 伪卡片的**淡出 + 展开动画**按人类口径「现在可以先不做」——留待有动画时一起做。
* 画布侧：`Shell / Browse / Libs Expanded` 那一帧目前还是「吃掉目录树」的旧口径，
  需要改成「目录树缩到最小值」—— **Pencil 掉线中断，等恢复后补**（`design/browse.md` 也要跟着改）。
