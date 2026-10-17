# 官网首屏下载按钮直达安装包文件

完成时间：2026-10-17 16:12:37 CST

## 背景（崔总 2026-10-17 真机截图）

截图里两个红箭头指向：首屏大按钮 `Download for Windows`，以及浏览器状态栏显示的
`github.com/C-Thun/raybend/releases/tag/v0.1.3`。也就是说 —— **首屏那颗最大的按钮点下去是
GitHub 的 release 页面**（还得用户自己在页面里再找安装包），而页面底部的下载区按钮才是
MSI 文件直链。崔总原话：哪个用户这么贴心，顶部大按钮不点，要滚到页面最底部点那个？

规格 `specs/website-homepage.md` §5 结尾早就写明：「**页面两处用它：hero 主按钮 + 下载区大卡**」
—— 实现只做了下载区一处，hero 一直无条件用 `releaseUrl`（= release 页面地址）。属实现欠账。

## 改动范围

| 文件 | 改动 |
| --- | --- |
| `website/src/data/release.ts` | 新增导出纯函数 `downloadHref(view)`：`available` → `downloadUrl`（文件直链）；`pending` / 无可识别资产 → `releaseUrl`。顺手把同一行里既有的 `[0]!.includes` 改成 `[0]?.includes`（`no-non-null-assertion` 告警，语义不变） |
| `website/src/sections/Hero.tsx` | 主按钮 `href={release().releaseUrl}` → `href={downloadHref(release())}`；底部说明从写死的 `earlyNote` 改为按发布状态二选一（有正式版显示 `readyNote`，带版本号） |
| `website/src/sections/DownloadSection.tsx` | 原来的 `available()?.downloadUrl ?? release().releaseUrl` 换成同一个 `downloadHref(release())`（同一能力只有一套实现） |
| `website/src/i18n/{zh,en}.ts` | `hero` 新增 `readyNote`：zh「最新正式版：v{version}；历史版本与校验值在发布页。」/ en「Latest stable build: v{version}. Older builds and checksums are on the releases page.」 |
| `website/src/data/release.test.ts` | 新增 `describe('downloadHref')`：直链 / 退化为发布页 / 未发布去发布列表页 |
| `website/src/sections/Hero.test.tsx` | **新增组件回归**（本仓第一条组件测试）：用与线上同形的 v0.1.3 资产注入 `currentRelease`，断言 hero 里存在 MSI 直链、且不存在 `/releases/tag/` 链接；另断言说明文字报出版本号 |

## 关键决策与理由

- **纯函数收口**：判断逻辑放 `downloadHref`，两处调用方共用 —— 正是这次出 bug 的根因形态
  （两处各写一份，只改了一处）。测试也挂在纯函数 + 组件两层。
- **说明文字一起改**：`earlyNote`（「目前处于早期开发，关注发布页即可」）在已发布后是错的，
  与「点按钮就下载」自相矛盾；改成按发布状态切换，与 `DownloadSection` 的 `pendingNote/readyNote`
  同构。
- **不改按钮文案与位置**：pending 态仍是「下载 Windows 版」指向发布列表页（规格 §1 的占位行为），
  不在本次动。
- 命令体系（应用侧纪律）不适用：这是官网静态站，没有命令注册表。

## 验证

- `website`: `tsc --noEmit` 通过；`vitest run` **28 passed**（原 24 + 新增 4：downloadHref 2 条 + Hero 2 条）；
  `oxlint src` 仅剩一条 Hero 既有的 `prefer-structured-class` 提示（非本次改动）。
- `RAYBEND_FETCH_RELEASE=1 pnpm build` 通过，产物里注入的是真实 v0.1.3 数据（含
  `RayBend_0.1.3_x64_en-US.msi` 直链），说明 CI 走 `/releases/latest` 的注入路径与本地一致。
- **未经人类验证**：线上效果要等推送后 CI 重新部署才能看到（推送归崔总）。验证方式：
  打开 https://raybend.cthun.com/ ，首屏「下载 Windows 版」按钮悬停时状态栏应显示
  `github.com/C-Thun/raybend/releases/download/v0.1.3/RayBend_0.1.3_x64_en-US.msi`，
  点击即开始下载。

## 遗留

- 官网首屏大按钮的 pending 态文案（无安装包时仍写「下载 Windows 版」）按规格原样保留。
