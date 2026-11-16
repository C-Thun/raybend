# M1-7 步骤 4：性能基线（M1 的第一份）

完成时间：2026-09-16 12:44:07 CST

## 环境（**不是真机，绝对值只能当参考**）

| 项 | 值 |
| --- | --- |
| CPU | Intel Core Ultra 9 185H（WSL 里可见 22 线程） |
| 系统 | WSL2（Ubuntu 24.04），软件渲染 |
| 盘 | **ext4**（`/tmp`，非 9p）—— 刻意避开 `/mnt/c` 的 9p 开销 |
| 构建 | `cargo build --release`（缩略图基准）/ `--example`（导入基准） |

> **为什么不用真机**：真机（Windows/WebView2/NVMe）的数字只能由人类在验收时得到（`AGENTS.md` §2.8）。
> 这份基线的作用是**追踪回归**与**判断量级**：同一台机器、同一载荷下，数字变差就说明代码变差了。

## 1. 导入吞吐（核心管线：扫描 + 规划 + 复制 + 入库 + 缩略图入队）

载荷：**5000 张合成文件，24 KB/张**（无 EXIF、非真图 —— 所以**不含**解码开销；
它量的是文件名分配、目录创建、复制、SQLite 事务与计数这条主干）。

| 场景 | 结果 |
| --- | --- |
| 首次导入 | **11.27 秒 → 443.7 张/秒，10.4 MB/秒** |
| 第二遍（同一批，判重） | **0.19 秒**走完 5000 张（约 2.6 万张/秒）—— 只扫描 + 比对，不复制 |
| 库一致性 | `assets=5000 asset_files=5000 import_items(imported)=5000 seq_counters=1` |

复现：

```bash
cargo build -q --example crash-drill
target/debug/examples/crash-drill seed /tmp/rb-scale 5000
target/debug/examples/crash-drill import-once /tmp/rb-scale/repo /tmp/rb-scale/src
```

## 2. 缩略图吞吐（GRID 384，JPEG q82）

载荷：**60 张真 JPEG，3000×2000，共 28.7 MB**（带纹理，不是纯色）。

| 项 | 结果 |
| --- | --- |
| 完整管线（读 → 解码 → 摆正 → 缩放 → 编码 → 写缓存） | **18.9 张/秒**；平均 52.8 ms、P50 51.9 ms、**P95 62.8 ms**、最慢 73 ms |
| 缓存命中路径 | **0.09 ms/张**（60 张 5.2 ms） |
| 输出体积 | 49 KB/张（384×256），压缩比 10.0% |
| 缩放算法 | **两段式 40.1 ms** vs Lanczos3 83.7 ms vs Triangle 28.5 ms vs Nearest 15.3 ms |
| 占位图（RAW 走这条） | 1.78 ms/张（只画图 + 编码） |

复现：`cargo run -q --release -p raybend --example thumb-bench -- <照片目录> --limit 60`

**判读**：编码是大头（P95 62.8 ms 里编码占~80 ms 量级的分阶段统计，见 bench 输出），
缓存命中路径快到可以忽略 —— 所以「缩略图卡不卡」取决于**首次生成**，以及 worker 池并发。
这份 bench 是**单张串行**跑的；应用里是 worker 池多线程（`thumbnail/worker.rs`），实际更快，
但这条基线足以追踪回归。

## 3. 网格滚动（纯逻辑，与渲染无关）

载荷：**50000 张**。

| 项 | 结果 |
| --- | --- |
| 分组（按拍摄日/时间片） | 55.9 ms（数据变化时算一次） |
| 行模型 | 13.8 ms（同上） |
| 行数 | 9100 |
| **窗口计算** | **72.6 µs/次**（1000 次共 72.6 ms） |

复现：`node --experimental-strip-types scripts/grid-bench.mjs 50000`

**判读**：窗口计算比一帧（16 ms）低**两个数量级**，所以 5 万张时滚动性能与数据量无关，
只跟 DOM 数量有关（虚拟化已把它压到常数）。

## 遗留

- 缺「真机 + 真实照片库」的数字（NVMe、WebView2、GPU）：属人类验收。
- 导入基线用的是**无 EXIF 合成文件**，所以它**不包含** EXIF 解析与 RAW 嵌入预览提取的开销；
  这两块的绝对成本要用真实照片量（人类验收时若体感偏慢，先量这两块）。
