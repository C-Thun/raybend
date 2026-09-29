完成时间：2026-09-29 19:05:53 CST

# browse 滚动稳定、右栏保持与 latest preview 复用

## 改动范围与原因

1. **tiles 点选跳顶/上移**：Solid 动态 props spread 使 focusRow 变化连带使 resetKey getter 失效，旧 effect 不核对目录键就 scrollTop=0；同时鼠标选中也被当成滚动焦点。现在目录键经过值去重，滚动 effect 明确订阅对应意图，focusId/focusRow 直接透传。鼠标点选保持 offset；键盘导航才发送滚入请求，按共享数据源的显示序取目标。查行优先读 timeline id，避免为定位遍历构造整库 GridItem。
2. **右栏异步中间态**：原先切图将 EXIF 与 issueLibrary 置空，Show 拆掉区块后再装回。复用 SelectedFileMetadata 增加带路径的 settled 结果，由 browse 的单份信息快照等待 EXIF/定稿两项结算再替换。空选择/换库/换目录立即清空；旧请求由 revision/cleanup 拦截。等待期间禁用旧快照的编辑入口并结束草稿，避免旧信息写入新照片。选中项 memo 也避免无关分页到达时重读当前 EXIF。
3. **film 总览/直方图**：总览单独持有最后一张完整 Screen 图及其尺寸，直到新 Screen 完成，图片 load 事件回填的最终尺寸也进入保持状态，不能拿 grid 裁切图替代；URL 在主图/总览间明确共享所有权，换图和关闭各回收一次。等待期间不绘制新照片的视野框到旧总览上。直方图加载期间保留上一组数据，失败/无选择才清空。
4. **库内 tiles 读取成本**：复用原有小图缓存键和写入逻辑，抽出 render_cached_with，依次使用小图缓存 → 同源版本/同基准/同管线版本的 latest preview 缩小 → 原片渲染。命中缓存或 preview 时不再解析镜头/LUT、不重新显影 RAW。preview 已含方向/编辑，只缩小，不二次套栈。减少一次写后重读与重复 stat。取消 browse 选中 JPG 时顺带读配对 RAW 的 EXIF；tiles 不再为缺宽高的旧 RAW 记录触发 metaEnsure。

## 涉及文件

- 滚动：src/components/ui/VirtualGrid.tsx、src/features/photo-grid/PhotoGrid.tsx、PhotoViewingStage.tsx、src/workspaces/browse/BrowseWorkspace.tsx。
- 信息：src/features/exif-strip/selected.ts 与测试；src/features/browse/info-snapshot.ts 与测试、BrowsePanels.tsx、ViewerReadout.tsx；src/components/ui/viewer/store.ts 与测试、HistogramPanel.tsx。
- 读取：src/features/browse/grid-source.ts 与测试；crates/raybend/src/thumbnail/render.rs、worker.rs；src-tauri/src/thumbs.rs。
- 回归：scripts/check-browse-boot.mjs 复用现有 CDP/假后端，新增 --stability-only；测试内容在 scripts/lib/check-browse-stability.mjs。
- 规格/记忆：specs/browse-stability-preview.md、memory/FUNCTION-BROWSE.md、memory/FUNCTION-IMAGING.md。

## 决策与复用

- 未改变布局/控件/Pencil 设计；修复的是既有组件生命周期。
- 未新建第二套 grid、EXIF 读取口或缓存实现；info-snapshot 只负责把既有异步数据组织成一份显示快照。
- 保留进入目录的磁盘同步和 catalog 配对数据，+RAW 无需读取 RAW 内容。新增/变化文件仍补录元数据，缓存缺失仍能重建；未给目录状态设置过期时间。
- 不新增命令或热键：现有点选、键盘导航、信息查看的修复，沿用已有命令注册。
- 本会话无可调用的 pi todo 工具；规格不作为另一个进度清单。

## 已验证（自动回归 / 冒烟）

- pnpm typecheck：通过。
- pnpm test：1119 项通过，含快照等待/失败/清空/换库、EXIF 乱序响应、总览保持与 URL 只回收一次、tiles 跳过 RAW 元数据。
- pnpm lint:colors / lint:arch / lint:i18n：通过。
- pnpm build：通过；保留既有大 chunk 提示。
- cargo test -p raybend thumbnail:: --lib：65 项通过，约 3.31 秒。新增不可解码的假 RAW + 合成 preview 验证，命中 preview 及小图缓存时若进入原片渲染即 panic；源替换可失效缓存；空/损坏 preview 仅回退一次。现存 develop.rs unused_mut 警告未改动。
- cargo check --workspace：通过。
- pnpm check:browse --stability-only：520 张合成照片，实际跨过 256 条数据页；上/中/下可见 tile 点击后 scroller 节点身份和 scrollTop 不变；tiles 与 film 分别延迟 EXIF/定稿响应，整份旧信息保持到两者完成；键盘仍能滚动到目标；无控制台错误和未处理异常。
- pnpm smoke:ui：通过，problems=[]。
- pnpm debug:win：最终构建通过（Windows cargo 约 3 分 20 秒），内置 check:win 通过；主程序时间 2026-09-29 19:05:10 CST，RAW worker 时间 19:04:19 CST，协议 raybend-worker-proto-v3，dist 的 4 个引用资源全部命中。产物：/mnt/c/rb-target/raybend/debug/raybend-desktop.exe。未执行 Windows GUI 交互验收。

## 环境绕法与验证边界

- 沙箱内 pnpm 的自身签名验证需要访问 registry，受网络限制而失败；通过工具提权联网执行 pnpm 验证和本地冒烟，未更改包管理器、依赖、锁文件或关闭签名校验。
- 自动回归只使用小规模合成数据，不代表真实照片库的 GUI、色彩或性能体感已验收。Windows 上滚动点击是否稳定、快速切图是否平顺、冷/热缓存速度仍由崔总确认。
- latest preview 缺失/损坏时按原有路径重建，首次仍可能读取 RAW；旧 RAW 记录若缺尺寸，tiles 暂用占位比例，明确打开看图/对比时再补读。
