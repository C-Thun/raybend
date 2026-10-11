# 缩图两档 + import 只在内存（本轮方案与验收清单）

> 崔总 2026-10-11 定。**「缩图规格有哪几档」「import（库外）的图存在哪」这两件事的唯一事实来源就是本文件。**
> 配套：`AGENTS.md` §2.24（库外/库内两套口径）、`specs/view-true-image.md`（看图显示真身）、
> `memory/FUNCTION-IMAGING.md` §2（库内三种图分工总表）。
>
> 一句话：**全局只剩两档缩图（thumb 384 / preview 1920）+ 真身；192（strip）这一档取消；
> import 的图一律只在进程内存里，view 里不要 1920，直接用 384 顶着等真身。**

---

## 1. 规格（改后）

| 档 | 长边 | 谁在用 | 库内（入库资产） | 库外（import / 未入库） |
| --- | --- | --- | --- | --- |
| **thumb** | **384** | tiles 网格、**所有**胶片带（browse / import / editor）、定稿卡片 | AVIF q90 + **落盘** `thumbs.db` | **JPEG q82 + 只在进程内存**（256 MB 字节上限） |
| ~~strip~~ | ~~192~~ | **已取消**（2026-10-11） | — | — |
| **preview** | 1920 | 库内：载入占位 / 总览 / 编辑过渡 | AVIF + 落盘（库根 `cache/full/`） | **没有这一档**（import 不生成、不请求） |
| **真身** | 该变体自己的像素 | view / compare / fullscreen 的**终态** | JPEG q92 + 落盘（库根 `cache/full/`） | 位图 = **原文件字节**（零渲染）；RAW = 完整解码 + **进程内存（10 张）** |

- **取消 192 的理由**（崔总）：384 拿着直接缩就行，没必要为胶片带多算一遍缩图。
  实测代价只是胶片带队列的字节从 ~1.3 MB 涨到 ~3.5 MB、屏上解码位图 98 KB → 393 KB/张（噪声级）。
- **import 不要 1920 的理由**：`specs/view-true-image.md` —— 看图显示的必须是真身，
  1920 只配当载入占位；而在 import 里 **384 就能当占位**，不必为占位多花 1.3–2.2 s 渲染一张 1920。

## 2. import（库外）的缓存口径

| 项 | 口径 |
| --- | --- |
| 存哪 | **只在进程内存**（`crates/raybend/src/thumbnail/memory.rs::SourceThumbs`），退出即清 |
| 落盘 | **一行都不写**（`_sources/thumbs.db` 只服务库内文件） |
| 键 | 路径 + 尺度 + 渲染签名 + **文件身份（大小 + mtime）** → 换卡 / 覆盖保存自动不命中，不需要失效钩子 |
| 预算 | 网格/胶片带档：**256 MB**（字节）；真身档：**10 条** |
| 前端的 blob 队列 | 另一层，管「同一屏来回滚不用重新要字节」；目录清单到手时按文件身份比对，变了的 `refresh(path)` |

## 3. 重采样

`render::resize_for_thumb` **去掉 Lanczos3 精修**，只用盒式快降（`imageops::thumbnail`）——
import 只求「最快出现」，画质可以让步。真身档与库内档同样走它（口径统一，不搞两套）。

> 下一步的提速（**登记 `memory/FUTURE.md`，不在本轮**）：libjpeg-turbo 的 1/8 DCT 缩放解码，
> 预期单张 200 ms → 30–60 ms（要新增 C 依赖 + Windows 静态库 + 许可登记，按 `AGENTS.md` §2.9 单开一轮）。

## 4. 验收清单

- [ ] `SizeClass` 只剩 `Grid / Screen / Full`；`ImagePurpose`、`ThumbSize`、`issue_thumb_get` 的白名单同步
- [ ] `render_sig` / `render_sig_source` 少一类分支；`memory.rs` 的小图档只认 `Grid`
- [ ] import 收尾只备 **grid**（不再 grid+strip 两遍）
- [ ] editor 胶片带与定稿卡片改取 384（显示尺寸不变）
- [ ] import 看图：占位 = 384 thumb，终态 = 真身；不再请求 1920
- [ ] 库外真身进内存（10 条）；位图真身仍走「原文件字节」短路
- [ ] `thumbs.db` 里已有的 strip 行清掉（一次性）
- [ ] 文档：`AGENTS.md` §2.24 / §11.5 / §6.5、`memory/FUNCTION-IMAGING.md` §2、`memory/FINISHED.md`、`memory/FUTURE.md`
- [ ] 质量门：`cargo test --workspace`、`pnpm typecheck && pnpm test && pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build`、`pnpm check:import`
- [ ] 真机：探针跑 `--source` 之后**磁盘缓存一行都没写**；单张 / 一屏成本记录到实施记录
- [ ] 人类真机确认：tile 观感（JPEG q82 + 盒式重采样）、胶片带观感（384 缩小显示）、import 看图的过渡
