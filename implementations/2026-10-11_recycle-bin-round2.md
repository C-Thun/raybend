# 库级回收站：第二轮真机问题修复（拖动统一 / 路径门禁 / 焦点 / 空目录）

2026-10-11 13:54:32 完成（构建中）。

崔总真机验收第一轮修复后，又报了一批问题。本文逐条记根因与修法。

---

## 一、拖动删除仍不起效 —— 时序 bug（**根因**）

`src/features/browse/photo-drag.ts` 的 `end` 回调顺序错了：

```ts
end: (point, cancelled) => {
  const final = current;
  current = null;
  setPhotoDrag(null);          // ← 预览一清，热区元素跟着卸载
  if (!started || cancelled || final === null) return;
  const target = hit(point);   // ← 这时 zoneRect() 已经是 null → 判不中 → 什么都不发生
```

命中判定用的是「工作区注册进来的热区矩形」（`registerDropZone`），而热区只在拖动时挂载 ——
**松手先清预览、元素卸载、ref 归 `null`，再算命中就晚了**。

**修法**：不只是换顺序 —— **整套命中机制换成 export 那套**（见下条「拖动统一」）：
`elementFromPoint` + `closest([data-...])`，直接问浏览器「松手这一刻指针下面是谁」，没有时序问题。

---

## 二、拖动统一（崔总点名：与 export 是两套实现）

**改前**：browse 与 export 各写一套内联预览 + 各自的路点判定 —— 尺寸（96 vs 132px）、透明度（无 vs 80%）、
命中方式（矩形注册 vs `elementFromPoint`）全不一样，「同一个手势看着像两个功能」。

**改后**（唯一实现，两边都用）：

| 能力 | 唯一出处 |
| --- | --- |
| 指针跟踪（阈值/帧节流/出窗中止） | `src/lib/pointer-drag.ts::trackPointerDrag`（原本就共用） |
| **落点命中** | `src/lib/pointer-drag.ts::dropHostAt`（新增；export 原来那套上移） |
| **预览卡片 + 数量角标** | `src/components/ui/DragPreview.tsx`（新增；export 的 JSX 上移并统一取值） |

取值（崔总 2026-10-11 定）：**尺寸用 browse 的（`size-24`）**、**透明度用 export 的（`opacity-80`）**。
删掉了 `registerDropZone` / `pointInRect`（及对应测试）与 export 的 `data-export-drag-*`
（冒烟脚本 `scripts/check-export-boot.mjs` 同步改用 `[data-drag-preview]` / `[data-drag-count]` /
`[data-drag-card]`，并把幽灵居中偏移断言对齐到实现值 `+8/+8`）。

**顺带修**：数量角标底色从 `bg-surface-layer`（浅色下是纯白，浮在照片上像界面外来的颜色；
深色下与 tiles 同色分不清边缘）**统一改为辅色 `bg-brand-2` + `text-fg-on-brand`**（崔总 2026-10-11）。

---

## 三、回收站视图/目录点击报「读不了这个库」

**现象**：展开回收站树 → 点里面的目录 → `读不了这个库：不支持：扫描范围必须是 photos 内的相对目录`。

**根因**：目录同步（`repository_sync_dir` → `rescan_scope` → `validate_scope`）只认 `photos` 前缀，
`recycle` 传进去直接报错；而 `BrowsePanels` 的 focus 同步把 `[PHOTOS_DIR, ...expanded()]` 整个送去，
**展开回收站时 `recycle` 就在里面**。`App.tsx` 的 liveRefresh 也把回收站 scope 送了进去。

**修法**（三处过滤 + 一道后端兜底）：

- `BrowsePanels`（focus 同步）：回收站范围直接跳过同步，只刷新树；`scopes` 里过滤 `recycle*`
- `App.tsx`（liveRefresh）：scope 是回收站时不同步
- `repo.rs`：`repository_sync_dir` 对回收站 scope **直接返回 `None`**（新 `is_recycle_scope`），
  `scopes` 列表同样过滤 —— 后台事件/旧页面送进来也不会炸

**原则**：回收站**不参与目录计数同步、不参与文件监听、不参与重建扫描**（它不是 `photos` 的子树；
扫描会把它当新资产对账）。计数只来自 `recycleSummary`。

> 附带说明（非 bug）：点「回收站」在 tiles 区能看到里面的照片，是因为它的范围与 `photos` 一致用子树语义
> —— 与点 `photos` 的行为相同（`scopePath = "recycle"`，照片来自 `recycle/<日期>/`）。文件在磁盘上的
> 位置是**镜像**的（`recycle/2026-09-13/MYP1000025.JPG`），不在回收站根下。

---

## 四、还原后回收站子目录行 / 分隔线残留

**根因**：`treeRows` 铺回收站子树的条件只有 `expanded().has(RECYCLE_DIR)`，
而入口行的显示条件是「有内容或正在看它」—— 还原掉最后一张后入口行消失、子树行还在。

**修法**：`treeRows` 的条件补齐为「（`count > 0` 或回收站模式）且已展开」；
分隔线放进入口行同一个 `<Show>` 里，跟着一起出现/消失。

---

## 五、确认弹窗的初始焦点（回车 = 前进）

**根因**：`Dialog` 没给 `initialFocusEl`，Ark 默认把焦点给**第一个可聚焦元素 = 右上角关闭叉**
→ **回车等于关窗**，用户以为确认了其实取消了。

**修法**：`Dialog` 新增 `initialFocusEl?: () => HTMLElement | null` 透传给 Ark；
`ConfirmDialog` 把焦点指向**「确定」按钮**。
口径写进注释：**回车 = 前进/确认，Esc = 关闭**（所有确认类弹窗一并生效）。

---

## 六、空目录触发式清理（photos + recycle）

**口径**（崔总 2026-10-11）：照片移走后不留空目录；**只在这些动作里触发**，不做主动扫描 ——
否则会跟用户自己新建的空目录打架。

**实现**：`recycle.rs` 新增 `prune_ancestors_of(from_dir, stop_at)`（**往上**逐级删空，`_RAW/` 一并覆盖）：

| 动作 | 清理 |
| --- | --- |
| 移入回收站 | photos 侧原位置往上收（`_RAW/` 空了也收） |
| 真删（purge） | recycle 侧往上收 |
| 还原 | 已有的 `prune_empty_dirs(recycle)` 递归收 |
| 清空 | 整个 `recycle/` 目录删掉（已有） |

**测试**：`prune_removes_emptied_dirs_but_keeps_user_made_ones` —— 同一目录里塞一个用户自建空目录，
断言「`_RAW` 被收掉、用户那个还在、目录层没被整层误删」。

---

## 七、_RAW / sidecar 边界核查（不改代码，补记录）

| 路径 | `_RAW/` 下的 RAW | 自家 `.xmp` | 别家的 `.xmp` |
| --- | --- | --- | --- |
| 删除（移入） | 随行（`asset_files` 有行，同批移动） | 随行（`rel_sidecar_of`） | 不碰（`is_ours` 守卫） |
| 还原 | 回原位（`create_dir_all` 重建 `_RAW/`） | 跟随改名（名字跟主体名） | 不碰 |
| 真删 / 清空 | 删除 | 删除（`is_ours` 才删） | 不碰 |
| 目录清理 | `_RAW` 空了由 `prune_ancestors_of` 收掉 | —— | —— |

---

## 八、AGENTS.md 纪律（本次新增/扩写）

- **§2.12 复用优先**（扩写）：动手前**先研判功能里有哪些可复用的子模块/子功能**；
  **画稿与既有实现冲突时既有实现优先**；❗**用户明确指定参考哪个功能时，全部精力用于复用那份参考，
  不许照着重做**（附 2026-10-11 拖动这件事的教训）。
- **§2.23 改动前做足影响调研**（新增）：软件越完善关联越紧，动手前列「会碰到哪些模块/调用方」，
  改完对清单核一遍，避免漏改（回收站这件事就是典型：扫描、门禁、目录树、缓存、XMP 侧车全被动到）。

---

## 九、验证（Agent 侧）

| 项 | 结果 |
| --- | --- |
| `cargo test --workspace` | **1490 + 133 + 7 + 2 + 2 全过**（`store::recycle` 12 条） |
| `pnpm typecheck && pnpm test` | **1277 全过** |
| `pnpm lint:colors / lint:arch / lint:i18n` | 全过 |
| `pnpm debug:win` | 已启动（产物 `C:\rb-target\raybend\debug\raybend-desktop.exe`） |

**未做**：真机 E2E —— 按崔总要求由他本人跑。

---

## 十、待崔总定夺

**AGENTS.md 压缩重构方案**（崔总 2026-10-11 提出）：见对话中的方案 —— 思路是「重新梳理」而非单纯压缩：
把散在模块文档里、但属于全局的内容（各类纪律、系统级设计规范、模块约定称谓、各类定义）上移到
`AGENTS.md`，同时把 §2 的纪律与 §11 的术语压缩成更紧凑的表达；模块细节文档不动。
方案确认后再动手（避免大改不合意）。
