# M1-5 B 批：照片网格（虚拟化 / 按时间分组 / 选择与排除）

完成时间：2026-09-16 04:56:08 CST

## 本次改动的范围

导入工作区**中列**：照片网格的虚拟化、按时间分组、选择与批量排除、缩略图加载队列，以及网格上方的档位控制条。

## 涉及文件

#### 纯算法（`src/lib`，全部带单测）

| 文件 | 内容 |
| --- | --- |
| `virtual-window.ts` | 可变行高的可见窗口：前缀和 + 二分查找，含 overscan |
| `time-group.ts` | 按拍摄日分组 + 「相邻间隔 > 1h 断为新片」的时间片；跨时区偏移安全 |
| `selection.ts` | 选择模型：替换 / 切换 / Shift 区间 / 反选 / 剪除（**与勾选是两套状态**） |

#### 组件

`src/components/ui/VirtualGrid.tsx`（自研，零新增依赖）、`Slider.tsx`、`src/features/photo-grid/{store.ts,PhotoGrid.tsx,GridControlBar.tsx,rows.ts,thumbnails.ts}`。

#### 接线

`src/App.tsx`（创建网格 store、`toolsbar` 的批量排除、单击选中 → 读 EXIF 喂 `flowbar`）、`src/shell/ToolsBar.tsx`。

## 关键决策与理由

1. **虚拟化自研，不引依赖**（Q1 定案）：固定行高 + 可见窗口 + overscan，纯函数在 `lib/virtual-window.ts` 里可单测。
   实测窗口计算 ≈30µs @30000 张（`scripts/grid-bench.mjs`）。没上 `react-window` 类库：Solid 生态没有对等且成熟的选项，而这里需要的东西不到 150 行。
2. **档位（tile 大小）用 `Slider` 而不是离散按钮**：连续调节才像「缩略图大小」这件事；档位记忆写进设置，跨会话保留。
3. **缩略图队列带「请求代次」**：快速滚动时旧请求的结果必须丢弃，否则会看到串图。
   并发 4 + LRU + `revokeObjectURL`（blob URL 泄漏会让长会话吃光内存）。
4. **双击/回车只做选中，不做查看器**：M1-5 不塞一个凑合的看图器进去；wgpu 查看器属 M2-W3（`FUTURE.md` G16）。
5. **Node 测试里 `createMemo` 只算一次**（solid-js 的 SSR 构建）：store 里因此**用普通派生函数**而不是 `createMemo` —— 否则单测里算出来的值和真实运行不一致。测试替身也必须与真 API 同形（别把时间字段清成 null）。
6. **批量排除用的是反选语义**：`toolsbar` 上的按钮操作的是「排除集」，与勾选/选中两套状态互不串味（`AGENTS.md` §11.3）。

## 验证方式

- `pnpm test`：三个纯算法的边界用例（空输入 / 单元素 / 越界 / 跨时区 / Unicode 路径）
- `scripts/grid-bench.mjs`：30000 张的窗口计算 ≈30µs
- `crates/raybend/examples/source-grid-smoke.rs`：真实样本 `/mnt/c/src/tmp/pic`（300 个文件，JPG+RW2 配对）跑扫描 → 行模型 → 缩略图
- `pnpm smoke:ui`：网格与 `toolsbar` 的禁用态

## 遗留问题

- 缩略图的**加载骨架**用的是 `Tile` 自带的 `loading`（脉冲），画布上没有对应帧（已记 `design/main.md` §9.5）。
- 真实照片库的体感、滚动手感仍归人类在 Windows 侧确认（`AGENTS.md` §2.8）。
