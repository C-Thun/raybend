# raybend 官网（`website/`）

raybend（光伴）的**官方站点**源码 —— 一个纯静态落地页：产品宣介 + 下载入口。

> ⚠️ **这不是应用本体。** 应用本体在仓库根（`src/` + `src-tauri/` + `crates/`，Solid 1.9 + Tauri 2）。
> 两者技术栈、依赖与发布流程都不同，**不要混用**（对照表见根 `AGENTS.md` §4）。

| | |
| --- | --- |
| 技术栈 | SolidStart 2（客户端静态形态）+ SolidJS 2 + Tailwind CSS v4 + Vite 8 |
| 产物 | `dist/client`（零服务端依赖的纯静态文件） |
| 发布 | GitHub Actions → GitHub Pages（`https://raybend.cthun.com/`） |

## 命令

```bash
pnpm install         # 依赖各自独立，必须在这里装
pnpm dev             # 开发服务器 http://localhost:3000
pnpm build           # 生产构建 → dist/client
pnpm serve           # 本地预览构建产物
pnpm test:run        # 单测（跑一次）
pnpm typecheck       # tsc --noEmit
pnpm lint            # oxlint src
pnpm icons:generate  # 重新生成 src/components/icons.tsx
```

## 文档

| 文件 | 内容 |
| --- | --- |
| `AGENTS.md` | **先读这个**：目录结构、i18n 约定、素材占位、下载信息注入、部署与自定义域名步骤、Solid 2 纪律 |
| `ASSETS.md` | 素材清单：要人出手的截图（尺寸/取景）与 AI 生图提示词 |

## 许可

**AGPL-3.0-only**（见仓库根 `LICENSE`）。第三方组件登记在根 `THIRD-PARTY-NOTICES.md` §1c。
