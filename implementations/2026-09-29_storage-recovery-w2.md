完成时间：2026-09-29 01:53:32 CST（UTC+08:00）。

# 可卸载存储 W2：来源恢复一致性

改动范围：`src/workspaces/import/{store,LeftColumn,ImportWorkspace}`、`src/features/dir-tree/{store,DirTree}`、`src/features/photo-grid/store` 及各 store 单测；`scripts/check-import-boot.mjs`、`specs/storage-recovery-w2.md`。未改 schema 或增加依赖。

复用 W1 App 卷观察器。卷变化后把最近、已勾选和当前目录合并单飞重查，再通知树和照片区；最近加载后自己核对，修复 hydrate 早于列表的启动竞态。路径键用现有 pathKey，缺少状态结果不能推定在线。计数代次挡住递归切换、移除再勾选和拔插后的旧结果；截断计数不当完整总数。

树保留展开状态，最多四个恢复读取在途，原始 Windows 根路径保留供 I/O 使用（规范键 `d:` 仅作比较，不用它读驱动器根）。恢复作废旧代次，并在原单飞读取后补读。照片区新增内部 refreshSource，离线作废迟到扫描/元信息，保留已有清单与选择；同路径恢复重读和刷新缩略图。排除与勾选在原共享 store 保留。未新增界面形态或用户命令，仍沿用批准的 W1 界面；自动恢复不登记命令/默认键，手动重试沿用原入口。

验证：全套 `pnpm test` 112 个文件级测试通过，0 失败（1.86 秒），新增启动晚返回、短状态数组、中文/NFC/大小写/分隔符、探测补读、旧计数、树迟到结果/并发上限、同路径恢复与选择保持用例。`pnpm typecheck` 最终通过；第一次因测试用了超出现有 TS lib 的 Array.at 失败，已改索引访问。colors/arch/i18n lint 和 `pnpm build` 通过，保留既有 chunk 提示。

保存的 `pnpm check:import http://127.0.0.1:1424/` 合成冒烟通过：卷退场、勾选保留、恢复后展开树重读和照片区同路径重扫，然后继续原确认导入/取消不开工/确认后开工流程；控制台异常与错误为空。第一次新增夹具的根路径多一个反斜杠导致等待恢复超时，修正比较路径后重跑通过，未放宽业务断言。日志 `/tmp/raybend-storage-w2-{tests-all,typecheck,build,import-smoke}.log`。

本机端口绑定在沙箱内返回 EPERM，已通过既有审批通道启动仅监听 127.0.0.1 的开发服务器及合成 Chromium 测试。仅做合成冒烟，Windows 内核真实拔插/慢盘/睡眠/DPI 和视觉未经崔总确认，四波完成后统一验收。W3/W4 界面补充稿已作并提交定案，不把它当真机验收。
