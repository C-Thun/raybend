# HEIC 格式全链路摸排（调研记录，暂不接入）

**完成时间：2026-10-09 23:44:44**

> 本轮**没有代码改动**：只做摸排 + 把结论记进文档。临时探针测试跑完即删，仓库回到干净状态。
> 结论汇入 `memory/FUTURE.md` §C10（唯一详细记述），并在 `memory/FUNCTION-IMAGING.md` 两处、`README*.md` 脚注做了现状修正。

---

## 1. 起因

检查「HEIC 现在能不能正常查看与编辑、导入时的小图能不能预览、导入后全链路是否正常」。
崔总随后拍板：**成本太高，暂不接入，只记录调研结果并汇进 FUTURE。**

## 2. 摸排结论（一句话）

HEIC 是「**能导入、能建索引、能读元数据，但像素链路全线断开**」——
小图没预览不是导入环节缺功能，而是缺 HEVC 解码器。

| 环节 | 现状 |
| --- | --- |
| 识别 / 导入 / 建索引 | ✅ |
| EXIF 元数据 | ✅ 能读（`kamadak-exif` 0.6 支持 HEIF 容器）；⚠️ 尺寸取不到（只在容器 `ispe` 盒 → 实测 `0×0`） |
| 导入 tiles / 浏览网格 / 胶片带 | ❌ 占位图（深底 + “IMG”） |
| 看图 / fullscreen | ❌ 退回网格占位图 |
| 编辑器 / 直方图 / 定稿 / 导出 | ❌ |
| 评级 / 标签 / 色标 / 集合 / XMP / 目录整理 | ✅ 不碰像素 |

**根因**：全仓唯一解码入口 `thumbnail::render::decode_file` 与 `color::input::decode_bitmap`
都基于 `image` 0.25，而 `image` 没有 HEIC 解码器（只有 AVIF）。

**为什么比 RAW 还难**：① HEIC 的内嵌缩略图是 HEVC 项而非 JPEG —— 没有 RAW 那种
「内嵌 JPEG 快路径」可以兜底出图；② Windows 上 Chromium / WebView2 不做 HEIC 解码
（https://github.com/MicrosoftEdge/WebView2Feedback/issues/1530 ），
所以连看图都必须走 Rust 解码，不存在 AVIF 那种「`<img>` 自己会解」的便宜。

## 3. 验证方式（实测证据）

- 临时探针（`crates/raybend/tests/zz_heic_probe.rs`，跑完已删）：直接调本仓真实入口，**不是**看文档推断。
- 样本 7 张（公开）：libheif `examples/example.heic`、nokia `content/images/{autumn,winter}_1440x960.heic`、
  `tigranbs/test-heic-images` 的 image1/image3、`tbraun96/heic-rs` 的 `with-exif`/`photo-2048`/`rotated-90`。
- 结果：`render_file(Grid/Strip/Screen)` 与 `display::pixels(Screen/Full)` **全部 `Ok(None)`**；
  `media::meta::read_photo_meta` 给 `0×0`；iPhone 样张真尺寸（3992×2992、4242×2828）只在容器 `ispe` 里。
- 命令：`cargo test -p raybend --test zz_heic_probe -- --nocapture`（探针已删，复现需重下样本重写）。

## 4. 关键决策与理由

| 决策 | 理由 |
| --- | --- |
| **暂不接入** | 成本形态与 RAW 相同（要自带解码器 + 缓存失效策略），却没有 RAW 的机型覆盖红利；而 HEIC 目前不是主要素材来源 |
| 将来要做时**首选 libheif + libde265** | 事实标准、最稳；构建照 dav1d 那套（`scripts/build-dav1d-win.cmd` / `scripts/lib/dav1d-win.mjs`）；纯 Rust 候选（imazen/heic、heic-rs、heif-oxide）列为备选，开工时先做对比 |
| 只记 FUTURE、不立 todo、不写 spec | 崔总明确「暂时没有做的计划」 |

## 5. 遗留问题（将来开工时的坑，已全部写进 FUTURE §C10）

1. **缓存失效**：HEIC 占位图已用正常签名（`avif-q90-*-v15-*`）写进 `thumbs.db`，
   接了解码器不处理也还是占位图 → 需 bump `PIPELINE_VERSION` 或定向清理。
2. **导入收尾会报错**：`prepare_source_images` 对 HEIC 报「无法生成 SOOC 预览」，
   缩略图任务重试 3 次后标失败（日志会刷；接线后自然消失）。
3. **尺寸 `0×0`**：可独立于解码器先做 —— 补 ISO-BMFF `ispe` 解析（与 FUTURE §B-5 的 CR3 尺寸同一个小活）。
4. **方向双转风险**：HEIC 同时有容器 `irot`/`imir` 与 EXIF `Orientation`，要确认只转一次。
5. **许可与文案**：接入时同步 `THIRD-PARTY-NOTICES.md`（libheif/libde265 ≈ LGPL-3.0）。

## 6. 未经人类验证的部分

- 本轮**没有在任何真实 iPhone HEIC 上跑过应用**：结论来自公开样本 + 本仓真实代码入口。
- Windows 真机上 `view` 的最终表现（占位图 vs 空态）未由人类目视确认 ——
  代码路径上 `ensureImage` 会静默 catch 失败、保留底色与网格小图，理应显示占位图。

## 7. 涉及文件

| 文件 | 改动 |
| --- | --- |
| `memory/FUTURE.md` | 新增 §C10（调研结论、候选选型表、开工清单、触发条件） |
| `memory/FUNCTION-IMAGING.md` | §1.4 heic 行的现状修正；§8 对照表 HEIC 行修正（写明看图/编辑/导出同样不可用） |
| `README.md` / `README.zh-CN.md` | HEIC 脚注：从「预览缺解码器」改为「显示与编辑都缺解码器，缩略图是占位图」 |
