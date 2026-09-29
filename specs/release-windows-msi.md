# WSL 一条命令生成 Windows 安装器

2026-09-29。承接 `memory/FUTURE.md` G22 与 2026-09-27 WiX 路径排障记录。

## 范围与方案

`pnpm release <test|patch|minor|major> --win-msi --unsigned` 在 WSL 发起，自动把构建输入同步到
Windows 探测到的 `%LOCALAPPDATA%\raybend\build\source`，Windows Cargo/Tauri 从本地目录构建和打包；最终产物仍收口到仓内
`release-out/`。`--win-nsis` 选择 NSIS，两开关可同时使用；旧 `--windows` 给出明确迁移提示。
不改变版本升迁、签名、worker、自定义协议、安装身份、上传与发布权限。

- 共用一套本地镜像和构建流水线，Windows 无需 Node/pnpm；dav1d 环境复用现有模块。
- 崔总追加：不能假定当前机器的盘符、用户名、WSL 发行版或 `/mnt` 挂载前缀。通过 Windows 已知目录探测 + `wslpath` 双向校验识别本地盘与实际挂载点；`--win-dir` / `RAYBEND_WIN_BUILD_DIR` 可指定 Windows 绝对路径或 WSL 挂载路径。执行前打印两侧路径，拒绝未挂载盘、网络盘、WSL Linux / UNC 构建目录和依赖当前目录的 `D:foo`。传给 Windows 的配置、target、文件型签名私钥和自定义 dav1d 目录使用统一转换。
- 复制 Cargo 清单、工具链文件、源码、资源、许可与本轮前端；不复制 node_modules、target、Git、官网和历史产物。
- 镜像归脚本管理，保留未变化文件的时间戳，删除已不存在的输入；拒绝未归属目录、符号链接和 Windows 路径冲突。
- 工作区锁 + 构建目录锁覆盖版本修改、前端生成、镜像、Windows 构建与收口；失败回滚版本并释放锁。
- 本轮 bundle 重新生成，finalize 只收选中的类型，避免同版本旧安装器混入。
- MSI 拒绝 beta / 带预发布标签的 test；保留 Windows 数值版本上限。
- 官网在同一版本同时存在两类包时优先 MSI，没有 MSI 时仍可下载 NSIS；不涉及视觉稿变更。
- 此功能属于开发发布脚本，不接入应用命令注册表，没有应用快捷键。

## 验收

- [x] 参数、通道、签名、回滚、锁竞争、镜像清理和产物筛选单测通过。
- [x] MSI、NSIS、双目标 dry-run 正确且不生成安装器、不升版；本机中文/空格目录两种路径写法回译一致。
- [x] 前端质量门和官网下载逻辑检查通过。
- [ ] 崔总实际运行一条命令生成 MSI，并完成安装、升级、卸载及 NSIS → MSI 切换验收。

实际打包、签名和发布由崔总执行（项目 AGENTS.md §2.1）；本轮 Agent 验证限脚本单测、只读工具链检查与构建冒烟。
