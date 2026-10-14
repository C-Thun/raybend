2026-10-15 03:07:12

# 首次正式发布（v0.1.2）复盘：四个坑与修法

崔总 2026-10-14 晚启动 `pnpm release patch --win-msi --unsigned --with-updater --publish`，
到 2026-10-15 凌晨全链闭合。这是仓库**第一次真正发布**（此前 `v0.1.1` 只在本地生成过产物、
从未上传过 Release），首次跑暴露了四处此前测试覆盖不到的问题。

## 结果

| 项 | 状态 |
| --- | --- |
| GitHub Release `v0.1.2` | ✅ 公开，6 个资产（msi + .sig + latest.json + 两个清单 + SHA256SUMS） |
| 更新稳定源 `releases/latest/download/latest.json` | ✅ HTTP 200（历史上第一次真正可用） |
| crates.io `raybend 0.1.2` | ✅ 已发布（重试后） |
| 官网 `raybend.cthun.com` | ✅ 显示 v0.1.2 + 下载直链（JS bundle 里核对） |
| 自动更新 | ⏳ 待真机验证（装 0.1.2 → 等 0.1.3；或 test 包手动检查更新看 0.1.2） |

## 四个坑（按出现顺序）

### 1. `versionEdits` 写死包名 → `raybend-crashpad` 漏同步（已修 `d3a03ac`）

`cargo metadata --locked --filter-platform x86_64-pc-windows-msvc` 拒绝写 lock：
「cannot update the lock file … because --locked」。根因：发版时只改了 `raybend` 与
`raybend-desktop` 两个包在 `Cargo.lock` 里的版本，2026-10-12 新增的第三个 workspace 成员
`raybend-crashpad` 没跟着改 → lock 与 manifest 不一致。

**修法**：`versionEdits` 改为按 `[workspace] members` 动态解析（读各成员 `Cargo.toml` 取包名
与 `version.workspace` 写法），新增成员自动跟上；lock 里缺条目或版本不符时点名报错。
回归测试钉住「新增成员漏同步必须报错」。

### 2. Agent 在构建期间改工作树（纪律教训，未改代码）

构建（01:19 起）进行中，我在 01:34 提交了磁盘报告改动（含 `package.json` 的 `clean:report`）。
发布阶段的两道校验因此失败：

- `sourceFiles` 快照校验（`源码已改变或缺构建快照：package.json`）；
- 版本文件比对（`版本文件含升版以外的改动`）。

而 `release.mjs` 的失败回滚又把「已提交内容」当成外部修改，冲突 → 抛 `AggregateError`
（真实原因被吞掉，只看到「构建失败；保留外部版本修改，需核对」）。

**修法（纪律）**：`AGENTS.md` §5.3 红线补一条 —— 构建/发布进行中禁止改工作树，
动仓库前先看 `.release/build.lock` 与镜像侧 `source.lock`。本次通过
`git reset`（把未推送的磁盘报告提交退回工作树）+ `git stash`（发布期间暂存）+ 快照对齐
（`git show base:…` + `versionEdits` 写入）恢复发布；发布后重放。

### 3. `releases/tags/<tag>` 对 draft 固定 404（已修 `940eaa4`）

`gh release create --draft` 成功后，脚本用 `GET /releases/tags/v0.1.2` 回读 → 404 →
误报「创建后未找到 Release」。GitHub API 的这个端点对 draft 不返回（列表端点才含 draft）。

**修法**：`readRemote` 改用列表端点 `releases?per_page=100` 匹配 `tag_name`；
夹具改列表语义 + 断言「不得再用 tags 端点」。

### 4. `cargo publish` 上传阶段 30 s 超时（已修 `940eaa4`）

`Uploading raybend v0.1.2` → `[28] Timeout was reached (Connection timed out after 30001 ms)`。
本机直连 crates.io 偶发（同时段我这边 curl 索引也超时过；几分钟后恢复）。打包与本地编译
校验都已通过，只差传输。

**修法**：发布时固化 `CARGO_HTTP_TIMEOUT=600` / `CARGO_NET_RETRY=5`（已有环境变量值不覆盖）；
文档也保留「失败无副作用、重试安全」的说明。重试后 0.1.2 进索引。

### 5. 官网 release 事件部署被 environment 规则拒绝（配置已修，文档已记）

deploy job 2 秒失败，注解：`Tag "v0.1.2" is not allowed to deploy to github-pages due to
environment protection rules`。`github-pages` environment 的部署规则只有 `branch: master`，
而 release 事件的 ref 是 `refs/tags/v0.1.2` → 被拒（此前 9 月的成功部署都是 push 到 master）。

**修法**：加 tag 规则 `v*`（`POST …/environments/github-pages/deployment-branch-policies`）；
之后 rerun 成功。文档 `docs/release.md` 一次性准备里补了这条。

## 发布脚本的断点重跑边界（这次学到的）

- **能用 `release:publish` 续跑**：工作树干净、且 `HEAD` 仍等于构建来源（或它是构建来源的
  直接子提交）。例：只创建了草稿、上传中断、crates 失败（在 crates 之前的几步都已完成）。
- **不能用 `release:publish` 续跑**：构建来源之后又出现了**新的独立提交**（`HEAD` 的父不再是
  构建来源）—— 校验会拒「HEAD 已离开构建来源」。此时要么重新构建，要么手工收尾。
  本次在手工收尾（`gh release upload` + `edit --draft=false` + `cargo publish`）之后才提交
  脚本修复，正是这个原因。
- 手工收尾时要**趁工作树还等于 tag 内容**时发 crate（`cargo publish` 校验工作树干净），
  否则 crate 内容与 tag 不一致。

## 顺带完成

- 磁盘报告（`pnpm clean:report`）与 `clean:win` 默认目标修正：`62f4e13`，发布期间暂存、
  发布后重放为 `c56f7b0`；实测占用与可回收量见 `docs/release.md` §12。
- 未推送提交：`940eaa4`、`c56f7b0` 由崔总 push。

## 遗留（需人类）

- 真机验证：装 0.1.2 → 建库/浏览/编辑/导出/卸载；`vN→vN+1` 自动更新（需再发 0.1.3 才能在
  0.1.2 上看到启动提示；或先用 test 包手动检查稳定源看 0.1.2）。
- `C:\rb-target` 旧构建目录 17.9 G 待清理（`pnpm clean:win --target-dir 'C:\rb-target\raybend' --keep-days 0`）。
- CI 打包仍按 `memory/FUTURE.md` G24 暂缓（触发条件：模型公开分发方式定了 / 有云签名）。
