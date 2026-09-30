完成时间：2026-09-30 21:58:30 CST（Asia/Shanghai）

# 相片整理上期：跨库多选导出交接

## 改动范围与文件

- `src/workspaces/export/store.ts`、`ExportWorkspace.tsx`、`Toolbar.tsx`、`src/App.tsx`：从标签／桶照片区进入导出时捕获整个选区；状态条显示待入队数；复用现有“送入队列”按钮与 `export.enqueue` 命令，逐库将每张照片的主定稿快照加入当前预设队列。画廊仍显示当前库，用户改选画廊照片或取消选中时放弃交接选区。
- `src/workspaces/export/store.test.ts`：跨库同号资产、主定稿选择、重复身份、130 张分批、离线库重试、异步清除／重置竞争，以及生产 runtime 路径的单元测试。
- `scripts/check-export-boot.mjs`、`scripts/check-browse-boot.mjs`：补齐新整理 IPC 与迁移快照的假后端响应，固定测试语言；旧工装先前把新命令返回 `null`／`undefined`，并受上次运行留下的英文偏好影响，造成与产品无关的误报。
- `src/i18n/{zh-CN,en-US}.ts`、`specs/photo-organization.md`、`specs/photo-organization-phase1.md`、`memory/{PLAN,FINISHED}.md`：双语状态与部分失败反馈、交接语义及上期进度。前一份主体实施记录的旧待决项指向本记录。

## 关键决定与理由

- 导出队列本来支持不同库条目，故沿用原队列与预设，不建第二套跨库导出器；只抽出一处快照入队函数，供原画廊及整理选区共用。`export.enqueue` 已在命令注册表中且默认键为 `Enter`，本次没有新命令或热键。
- 整理选区以 `(repoId, assetId)` 去重；每张只选当前主定稿，不把其它 issue 意外放大为多份输出。每次入队按当前库连接状态和库根逐库读快照，单批最多 128 张；队列按库 ID、资产 ID、内容哈希去重。
- 入队失败不丢选区：已经在队列的照片从交接选区移出，其余保留并显示错误，离线库恢复后可重试；清除选中或重置期间的旧异步响应不能重新入队。仅由显式“送入队列”动作提交，不因切到导出自动启动任务。
- UI 使用导出状态条既有 `countLabel`，未新增版面或控件；既有 Pencil 定稿无需改布局。Windows 真机交互和视觉仍属人类验收。

## 验证

- 前端 `tsc --noEmit`、Node 单元测试：123 个测试文件通过；颜色、分层、i18n 静态检查与 `git diff --check` 通过；最终 Vite 构建通过。
- 可重复 `scripts/ui-smoke.mjs`：`problems: []`；`scripts/check-export-boot.mjs` 与 `scripts/check-browse-boot.mjs` 通过。先前两条启动脚本因假后端缺新 IPC 响应及语言状态不固定而失败，补 fixture 后复跑通过。
- Windows debug 主程序重构建后，`scripts/check-win-artifact.mjs` 核对嵌入的 4 个前端资源全部命中，独立 worker 协议为 v4。未运行 release、打 tag 或推送。
- 本轮只改前端交接与测试工装；主体 Rust 单元 `1246` 通过、`6` 忽略以及 `cargo check --workspace` 的结果沿用同日主体实施记录，未修改 Rust 源码后重复重跑。

## 遗留与验收边界

- Agent 已完成代码、单元、浏览器和构建冒烟，**未声称 GUI E2E 已验收**。崔总需要在 Windows 真机用两个已挂载库（含相同整数资产 ID）验证标签／桶多选 → 导出队列、离线库恢复后重试，以及真实照片的窗口/DPI 与大库性能；完成后 Agent 应自行复验并补记录。
- AI 标签与文字遮罩仍属于下期，未实施。
