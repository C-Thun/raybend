# M3-W3：显影管线 + 编辑栈 + 缓存与 issue

完成时间：2026-09-24 04:27:14 CST

> 本波**跨了一次会话**（前一段做了管线/落库/曲线/撤销，后一段做缓存与 issue）。
> 上游：`plans/M3-W3.md`（本波计划）、`plans/HANDOFF-2026-09-23-w3.md`（换手文档）。
> 四个口径由人类 2026-09-24 当场拍定（见 §1）。

---

## 1. 这一波做了什么（按提交）

| commit | 内容 |
| --- | --- |
| `8e8f89b` | 显影管线（`crates/raybend/src/develop/`）+ 编辑栈落库（`catalog_0005`）+ RAW 线性解码 + 渲染线程接入 + 前端实时预览 |
| `ab51034` | 编辑可撤销（复用 `browse` 的撤销栈）+ 二级锁禁用 + 撤销后重读 |
| `660ed9a` | 曲线编辑器（自研控件）+ 跨语言测试向量 |
| `d65cb5c` | 覆盖层变换契约（Rust 出矩阵）+ 重置全部调整命令 |
| 本次收尾 | AVIF 缓存格式、库内大图缓存、issue 解析、看图/缩略图接 issue、as-shot 基线入库 |

### 1.1 人类当场拍定的四个口径

1. **参数生效**：管线结果 = 一张静态图；GPU 视口只显示静态结果。**不做**「1:1 先看糊图、松手才变清」的妥协。
2. **scene-referred 一步到位**：RAW 走**完整解码 + 去马赛克**（rawler 的 PPG），去掉 `SRgb` 步拿线性。
3. **issue**：本轮只有一个存储位 `latest`；`SOOC` / `RAW` 是虚拟 issue（有 JPG / 有 RAW 才有）。
4. **色温 = 绝对 K**：载入时标尺挪到照片自己的 as-shot 色温；RAW 真绝对、JPG 表面绝对（本质相对）。

### 1.2 关键实现（落点）

| 事 | 在哪 |
| --- | --- |
| 参数表（唯一真相） | `src/api/develop-params.json` ↔ `develop/params.rs`（两侧逐条断言） |
| 色彩数学 | `develop/color.rs`（sRGB 传递函数、Kelvin↔xy、白平衡增益、CCT 反查） |
| 曲线 | `develop/curve.rs`（单调三次 Fritsch–Carlson，**端点可左右拖 = 黑场/白场**） |
| 管线 | `develop/pipeline.rs`（线性 u16 → 显示 8bit；参考实现 + 3×4096 LUT + 按行多线程） |
| RAW 线性解码 | `raw/rawler_backend.rs::LINEAR_STEPS` + `worker.rs` 的 `linear16` 协议 |
| 编辑栈 | `store/develop.rs` + `store/migrations/catalog_0005_develop.sql` |
| 撤销 | `store/marking.rs` 的 `Op::DevelopParam` / `Op::DevelopCurve`（**值走 f64 位模式**保 `Eq`） |
| 渲染线程 | `src-tauri/src/editor.rs`：解码线程 → **显影线程**（线性源常驻 + 最新者优先） |
| 大图缓存 | `display/full_cache.rs`（`<库根>/cache/full/<资产>/<issue>-v6.avif`） |
| issue 解析 | `store/develop.rs::choose_issue` / `edit_target` |
| 缓存格式 | `thumbnail/render.rs::encode_avif`（AVIF 90 / 4:4:4 / 速度 10） |

---

## 2. 实测数字（本机 WSL，dev profile `opt-level=2`，22 线程）

### 2.1 管线（`cargo run -p raybend --example develop-probe`）

| 场景 | 耗时 |
| --- | --- |
| 预览档 1920×1080（2MP）默认参数 | 8.2 ms |
| 预览档 · 带色度（饱和/自然饱和） | 13.4 ms |
| 1:1 档 6000×4000（24MP）默认参数 | 74.8 ms |
| 1:1 档 · 带色度 | 125.9 ms |
| 24MP → 1920 箱式降采样 | 126 ms（`image` 的 thumbnail 要 302ms、Triangle 要 845ms） |

### 2.2 RAW 线性解码（真机样本 `P1000019.RW2`）

5184×3888 → 线性 u16（60,466,176 个值），**拍摄色温 4350K**，端到端 **约 1.6 s**
（其中 worker 内约 1.2s：开文件 320ms / raw_image 45ms / develop 430ms / 转换+编码 395ms；
余下是 120MB 跨进程搬运）。

### 2.3 缓存格式（`cargo run -p raybend --example avif-probe`）

| 尺寸 | AVIF 90（线程 / 速度 10） | JPEG q82 | 体积对比 |
| --- | --- | --- | --- |
| 384×288（网格） | 62 ms | 5 ms | 5 KB vs 12 KB |
| 192×128（胶片带） | 38 ms | 1.6 ms | 1 KB vs 3 KB |
| 1920×1280（看图） | 539 ms | 100 ms | 115 KB vs 270 KB |
| 2560×1707（大图） | 904 ms | 193 ms | 208 KB vs 481 KB |
| 5184×3888（全尺寸） | 4.8 s | 653 ms | 913 KB vs 2215 KB |

**结论**：体积省 2.3–3 倍，编码慢 6–12 倍。所以（a）`image` 的 `rayon` feature 是必须的
（不开线程慢 8 倍以上）；（b）缩略图那条队列必须留在后台；（c）速度档位取 10
（速度 8 只省 20–30% 体积却要 3 倍时间）。合成图是高频噪声，真实照片会更快。

---

## 3. 验证方式（跑了什么、看到什么）

```bash
npx tsc --noEmit -p tsconfig.json      # ✅ 干净（判据以它为准，pi-lens 的 LSP 缓存会落后）
pnpm test                              # ✅ 895 条
pnpm lint:colors / lint:arch / lint:i18n # ✅ 三条全过
pnpm build                             # ✅
cargo test -p raybend --lib            # ✅ 904 条（含 develop 34 / store::develop 15 / full_cache 8 / cache 17）
cargo test -p raybend-desktop --lib    # ✅ 65 条
cargo clippy --workspace --all-targets # ✅ 只剩 store/backfill.rs 那条既有警告
```

**像素与耗时证据**（都能重跑）：

```bash
cargo run -p raybend --example develop-probe -- /mnt/c/src/tmp/pic/P1000019.RW2 /mnt/c/src/tmp
cargo run -p raybend --example avif-probe
```

**外部给定的期望值**（不是「跑一遍记下来」）：

* 管线：曝光 ±1EV 的中灰输出（162 / 85）、色温方向、饱和 −100 变灰 —— 手算写死在测试里；
* 曲线：`src/lib/curve-vectors.json`（Rust 生成，Rust 与 TS **两侧都对着它断言**）；
* 覆盖层矩阵：与 `image_to_physical` 同源、zoom/dpr 比例、无洞口返回 `None`；
* AVIF：`is_valid_avif` 认 ISO-BMFF 的 `ftyp` box。

---

## 4. 明确没做（不是遗漏，是排期）

| 事 | 去哪 |
| --- | --- |
| LUT、多 issue / issue 改名、变更集与缓存快照重整 | **M3-W6**（人类 2026-09-24 重排；原 W6 收口顺移 W7） |
| 清晰度（降噪/锐化）、镜头校正 | M3-W4（界面已禁用并写明，不会假装能用） |
| 裁切/旋转/对比的画布交互 | M3-W5（覆盖层宿主与矩阵契约已在 W3 立好） |
| 去马赛克的更高质量算法（RCD/AMaZE） | 定案记录：**v1 用 rawler 的 PPG**（快、稳、够用）；升级登记在 `FUTURE.md` D2 |
| XMP 的读写实现 | 只定契约（真源是 DB）；需要互操作时再做 |
| 编辑后**直方图**（曲线背景那层仍是 SOOC 的直方图） | W4 —— 需要 `image_histogram` 接编辑栈 |
| 大图缓存的 GC（按容量/版本回收孤儿） | W6/W7（现在只按 `v<管线版本>` 自然失效 + 编辑后 `invalidate`） |

---

## 5. 遗留问题与风险

1. **换照片会重新解码**：显影线程只缓存当前一张的线性源，切走再切回来要再解一次（~1.6s/20MP）。
   预取邻图 / 多缓存是后续优化项。
2. **编辑器加载一张 RAW 约 1.6s**（质量优先的代价）：这是「1:1 不糊、两档颜色一致」换来的。
3. **AVIF 编码成本**（§2.3）：导入 1000 张的缩略图从 ~5s 变 ~60s（后台队列，不挡导入）。
   若人类觉得小图不值得，把 `thumbnail/render.rs` 的编码换回 JPEG 即可（一处）。
4. **缩略图队列不跨工作区共享**：编辑器里编辑完会清掉自己的缩略图队列，浏览侧的网格要等
   重新进入目录才刷新（或由将来的全局失效机制处理）。
5. **`develop_params` 只存非默认值**：所以「默认值改了」不会影响老数据（这是刻意的，见迁移注释）。
6. **`as_shot_k` 存在 issue 里**：这是新加的列（同一波内改的 `catalog_0005`，未发布所以不写迁移）。

---

## 6. 给人看的验收清单（E2E 归人类，`AGENTS.md` §2.8）

最短门槛三步（Windows 产物）：

1. 进编辑 → 看到照片；
2. 拖「曝光」→ **画面跟着变**（松手落库）；
3. `Ctrl+Z` → 变回去。

再往下（同一条清单的继续）：

4. 曲线：点线上加点 / 拖动 / 双击删点 / 拖两端方块（黑场白场）/ 重置通道；
5. 色温：载入 RAW 后标尺**自动落在照片自己的色温上**（本机样本约 4350K）；
6. 换到浏览 → 该照片的缩略图/看图**显示编辑结果**（大图缓存在 `<库根>/cache/full/`）；
7. 编辑过的 RAW：非编辑器全图查看应当命中大图缓存（第二次打开明显更快）；
8. 二级锁的照片：右栏参数整列禁用并写明原因。

<!-- 列表被中间那句说明打断过，这里把编号接上，别让 markdown lint 认为编号跳了 -->
