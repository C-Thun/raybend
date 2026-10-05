# 独立模型仓库与可选 AI 构建方案

> 2026-10-04，崔总已提供相邻 `model-registry` 并授权实施；已完成通用模型库、固定 CPU 导出与项目本地源登记，可选 AI 构建和无 AI 设置状态已接线。无 AI 稿 `design/main.pen` 的 y02Ugg 已获确认。验证证据和未测项见本轮实施记录；不生成安装器或推送。
> 本方案替代上一轮尚未交付的“软件打包自动获取 ONNX 地址”接线；首次只支持显式登记的本地模型库，普通构建不拉取原始权重、不运行模型导出、不自行更新模型仓库。

## 1. 决定与范围

独立仓库采用崔总提出的 `model-registry`，作为多个项目共同消费的模型制品库，目录与元数据协议不绑定 RayBend。模型独立发布、独立版本；消费项目各自声明兼容契约和准入摘要，不能以仓库的最新目录自动替换模型。

当前 TinyCLIP 图像编码器 158,885,258 bytes（151.53 MiB）。如果托管在 GitHub，普通 Git 不能推送超过 100 MiB 的单文件，所以按崔总“导出后自动提交”的要求采用 **Git LFS**：Git 提交文件指针，本地检出保留完整 ONNX，推送时再上传 LFS 对象。依据：[GitHub 大文件限制](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github)、[Git LFS 工作方式](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-git-large-file-storage)。

公开仓库并不是本地构建的前提；只要本地包完整且受信就可构建。公共下载可以后续用独立仓库的模型 Release 附件提供，模型版本不跟 RayBend 版本编号走。本波不自动创建远端仓库、打 tag、push 或上传；这些操作归崔总。

只推进一个工作单元：模型库初始化、TinyCLIP 导出入口、本地源登记、统一 AI 构建决定、所有构建入口和产物核对。不新增模型、不重新调阈值、不做应用内 HTTP 安装、不扩充识别类别。

## 2. 两条流程

**模型制作**：确定通用模型库 → 检查归属/并发/Git 条件 → 检查已有固定制品 → 必要时准备隔离导出环境与固定上游权重 → 导出并做 CPU 对照 → 组成通用模型制品与项目消费配置 → 原子发布目录 → Git 库中提交本次文件 → 由项目入口原子登记本项目的本地源。

**应用构建**：读取本地源 → 验证包和兼容性 → 准备固定平台 CPU 运行库 → 冻结本次 AI 开关与输入 → 构建前端/Rust → 仅装配本次需要的资源 → 核对构建身份与模型资源。

普通应用构建只消费成品，不调用导出流程，不修改模型库，不自动 `git pull`、`git lfs pull` 或提交；LFS 指针尚未下载时明确判为模型不可用。

## 3. 模型库布局与身份

建议用三级目录定位模型：**格式 → 模型标识 → 导出版本**。第二级是稳定模型标识，第三级集中表达上游 revision、opset、精度和导出配方版本；不以软件版本或单独的 PyTorch 版本命名。例：

```text
model-registry/
  .model-registry.json        # schema / kind / registryId，纳入 Git
  .gitattributes              # onnx/**/*.onnx 使用 LFS，纳入 Git
  .gitignore                  # 忽略 .staging/、锁、工具环境与原始权重缓存
  readme.md                   # 创建/导出/检出 LFS/发布说明（按崔总命名）
  onnx/
    wkcn-tinyclip-vit-40m-32/
      hf95ec8197-op17-fp32-r1/
        artifact.json         # 通用模型契约、上游身份、文件大小/摘要
        image_encoder.onnx    # LFS
        LICENSE.txt           # 上游许可与派生说明
        export.json           # 导出工具版本、配方摘要、对照或导入来源记录
  profiles/
    raybend-photo-tags/
      v1/
        profile.json          # 引用上述模型路径与完整摘要
        manifest.json         # RayBend 现有安装包契约与可信身份
        classes.json          # RayBend 标签向量、阈值
        LICENSE.txt           # 当前 RayBend 标签包的许可/说明
  .staging/                   # 验证成功后才发布到正式目录
```

`onnx/` 下只存通用模型制品与必要契约；同一个图像编码器供多个项目复用。业务词表、标签向量、阈值放独立 `profiles/`，明确绑定模型的完整摘要，避免把 RayBend 的七类标签写成模型本身的通用能力。可增加其它项目的 profile，不复制 ONNX。ONNX Runtime 平台 DLL 仍由消费项目的构建脚本管理，不混进平台无关的模型目录。

三级目录是人可读定位符，**完整 SHA-256 是精确身份**；目录中的 revision 短写不替代元数据中的完整上游 commit。`hf95ec8197-op17-fp32-r1` 表示固定上游版本、opset17、FP32、导出配方r1；变更权重、精度、opset或导出配方需新版本，已发布目录和 profile 都不可原地覆盖。工具版本和配方详细依赖写入元数据，不在路径里塞满所有版本号。时间/测速记录不进入稳定身份，同一结果重复运行不产生新版本。

所有目录段使用有长度限制的小写 ASCII 标识；明确许可/源身份避免名称碰撞。单个模型超过一个 ONNX 文件时，相关文件仍放同一第三级目录并全部列入 artifact.json，不再加多级路径才能定位；本波只交付当前单编码器。将来可新增其它格式顶层目录，当前不实现其工具。

根标记采用 `schema: 1`、`kind: "model-registry"`、随机 UUID `registryId`。它用于判定目录用途，**不替代模型摘要和项目兼容验证**。库中不保存引用项目的绝对路径，不要求其它项目读取 RayBend 的代码或配置。Git 根通过 `git rev-parse --show-toplevel` 判断，支持 `.git` 为文件的正常 worktree，不凭是否存在 `.git/` 猜测。

通用导出脚本使用上述格式/模型/版本契约，不硬编码项目名；项目入口另外选择 profile、登记本机引用并装配平台资源。当前导出实现仍从 RayBend 已有脚本收敛，通用导出与项目包装分清职责、只保留一套实现；本波不为了共享读取引入包发布平台或第二套导出器。

RayBend 构建时把引用的 ONNX、profile 的 classes/manifest/license 组合到本次暂存，恢复现有四文件安装包形状，校验现有已认可 manifest 摘要。这样能共享基础模型并保持当前推理/安装协议；profile.json 的模型绑定必须与 artifact.json 和实际编码器字节一致，不允许修改 profile 使不受信模型获得准入。通用模型正确性与各项目的类别效果准入分开判断。制作 RayBend profile 复用已归档配置与校准结论，不重新下载 1,294 张样本调阈值。

## 4. 路径与初始化

已实现入口：

```bash
# 不传路径：建立或复用专用本机模型库，不初始化 Git
pnpm ai:export

# 崔总先创建、clone 独立仓库后，只需一次初始化
pnpm ai:library:init -- /absolute/path/to/model-registry
pnpm ai:export -- /absolute/path/to/model-registry

# 已 clone 且已有完整包，只做登记，避免为了换机器重复导出
pnpm ai:use -- /absolute/path/to/model-registry
```

默认路径是工具专用数据目录，与应用安装后的 `ai-models` 和可删图片缓存分开：

- Windows：`%LOCALAPPDATA%\model-registry\library-v1`。
- Linux / WSL：有效绝对路径 `$XDG_DATA_HOME/model-registry/library-v1`，否则 `$HOME/.local/share/model-registry/library-v1`。

默认库不存在时可以创建并写归属标记；同名默认目录没有标记时，仅允许显式初始化规则中的空目录或新仓库；未知非空目录拒绝写入。传入路径必须已经是带标记的库，导出脚本不能暗自接管任意目录。

`ai:library:init` 是唯一显式初始化入口：允许新目录、空目录、或仅含 `.git`、README、LICENSE、合法 `.gitignore/.gitattributes` 的新仓库；先全量检查允许的少量根项，再初始化。已有其它数据、错误归属标记、根为链接/系统目录、模型库与 RayBend 源码/构建目录重叠，均拒绝。不能覆盖已有文件；已有 attributes 冲突、hook 冲突不强制改写。

路径先绝对化与规范化，再校验真实父目录和最终目录；限定所有产物在库根之内，拒绝包路径的 `..`、绝对路径、特殊文件与链接逃逸。支持空格、中文和正常 Windows 路径；shell 参数使用参数数组或现有 Windows 路径适配器，不拼接不可信命令文本。

## 5. 导出、复用与失败处理

1. 单写锁覆盖一次导出；已有有效包可直接读取。锁有 PID/开始时间，遇到占用报错，不未经确认删“旧锁”。
2. 先检查目标包：完整、摘要和兼容性一致就复用，不重复下载/导出、不制造空提交；同名包损坏或内容不同就失败，保留原目录，不覆盖修复未知数据。
3. 缺少固定包时，才准备已锁版本的 Python/CPU PyTorch/Transformers/ONNX 导出工具环境和上游固定 revision。环境位于工具专用目录；不污染全局环境，不进入应用或模型 Git。CPU 完整可用，无 CUDA 要求。首次网络/磁盘成本只发生在制作端。
4. 编码器导出统一迁入 `model-registry/scripts/export-tinyclip.py`；RayBend 原研究入口只调用它并生成研究用文本向量。复用已有标签配置和校验/包装实现；锁定所有会影响导出结果的工具版本并记录配方摘要。验证 ONNX I/O、CPU 加载、有限输出、PyTorch/ONNX 对照和文本向量配套。
5. 当前 v1 包继续使用已认可的固定 manifest 摘要。即使同一权重重复导出，也不假定字节必然一致：若固定配方产生不同摘要，保留明确标为未准入的制作结果与对照报告，失败退出，**不自动注册成生产模型、不自行改可信列表**。
6. 只在 `.staging/<随机ID>` 写入；通用模型与选定 profile 均验证后发布到其不可变目录。本次提交完成后，项目入口才登记引用。失败只清理本次暂存，不删除现有模型，不修改旧源指针；其它项目的制品/profile 不动。应用包不夹带原始 PyTorch 权重、导出环境或样本照片。
7. 模型库可复用已导出并核对的 v1；实施时优先把当前现成包引入独立库，避免为了搬家重复跑大下载，但仍必须交付真正可从零运行的导出脚本。

## 6. Git 自动提交

仅当传入库本身是 Git 根时自动提交；不能因某个祖先是 Git 仓库而把结果提交到它。非 Git 库正常产出、正常登记，无提交。

写入前检查可写分支（允许新仓库尚无首个 commit）、无 merge/rebase、提交身份已配置、LFS 已安装且 ONNX 规则有效。初始化使用仓库局部 LFS 配置，不能改全局 Git 配置、不能强制覆盖已有 hook。

自动提交前要求没有无关的已暂存/未暂存/未跟踪改动；中途检查 HEAD 与 index 是否发生外部变化。只 `git add --` 本次初始化或模型包的明确路径，再提交中文消息，如 `feat: 添加 TinyCLIP 标签模型 v1`。绝不用 `git add .`、`git add -A` 或自动 push。

提交失败保留已验证的产物，**不更新软件源指针**，脚本非零退出并说明产物仍在。修复身份/LFS等问题后，重跑可验证并提交上次本脚本留下的完整包；恢复仅允许明确归属于该包的变更，不能把外部修改混入，也不 reset/清空外部 index。

已有完整且已提交的包重复运行无需新 commit；HEAD 中应只有 LFS 指针，本地 `.onnx` 必须是实际模型字节，不把未下载的指针当成 ONNX。

## 7. 本项目的本地源登记

建议文件为项目根 `ai-model-source.local.json`，**仅此本机文件加入 `.gitignore`**，不提交绝对路径。它是本地选择记录，不是可信来源配置；示意：

```json
{
  "schema": 1,
  "kind": "local-library",
  "registryRoot": "/absolute/path/to/model-registry",
  "registryId": "<库的UUID>",
  "artifact": "onnx/wkcn-tinyclip-vit-40m-32/hf95ec8197-op17-fp32-r1",
  "artifactSha256": "<artifact.json完整摘要>",
  "profile": "profiles/raybend-photo-tags/v1",
  "profileManifestSha256": "<当前已审阅标签包摘要>"
}
```

`ai:export` 的项目入口在导出成功（Git 库还需提交成功）之后原子写入该文件；`ai:use` 对已存在的模型和 RayBend profile 执行同样校验后登记，不需要导出或修改模型库。其它项目保存各自的本地引用与兼容要求，不由导出器遍历并改写项目配置。

模型库搬家或另一台机器检出时执行 `ai:use` 更新本地登记；首次仅支持本地路径，配置内不保存账号、token、Git 凭据或虚构 HTTP URL。普通打包不隐式发现某个默认目录，也不偷偷使用软件仓内此前残留的 ONNX。

## 8. 统一可选构建决定

在前端构建和版本写入之前，复用一个解析器产生本次 `AiBuildPlan`（enabled/reason/包身份/快照资源/feature 列表）；debug、dev、release、产物核对共用，不能各写判断。判定顺序：

1. 构建模式；`off` 直接关闭且不访问源/网络。
2. 本地源文件是否存在、是否为有界合法 JSON、schema/字段是否受支持。
3. registry 归属标记和 UUID 是否匹配，artifact/profile 相对路径是否受限，所有固定文件是否已实际检出。
4. 逐文件有界读取大小与 SHA-256，artifact/profile 绑定是否一致，组装的 RayBend manifest 是否受信，I/O/预处理/词表/阈值/ORT 版本是否匹配本项目契约；不能仅因 registry 认可通用模型就替消费项目跳过兼容验证。
5. 准备已固定的平台 CPU 运行库：复用已有本地导入/校验、官方运行库缓存；必要时仅下载官方 ORT 固定制品并校验。**不下载 ONNX、不导出、不修改模型库**。CPU 运行库不能默认从系统 PATH 或 CUDA 安装中找。
6. 冻结验证过的包到本次构建暂存，再验证拷贝摘要；后续库移动/修改不得改变正在构建的输入。CPU DLL 任一缺失、损坏或获取失败，算 AI 资源不可用。

默认 `auto` 符合崔总要求；另外提供 `required` 给需要确保 AI 的正式构建和 CI，`off` 用于明确构建基础版：

| 场景 | `auto`（默认） | `required` | `off` |
| --- | --- | --- | --- |
| 无本地源文件 | 提示，继续构建无 AI | 构建前失败 | 无 AI |
| 配置错误/路径失效/未下载 LFS/包不受信/资源损坏 | 提示具体原因，继续无 AI | 构建前失败 | 无 AI，跳过检查 |
| 模型与平台 CPU 运行库完整可用 | 包含 AI | 包含 AI | 无 AI |

例：`pnpm debug:win -- --ai=required`；release 接收同一模式选项。前端单独 `pnpm build` 不宣称生成完整 AI 桌面产物；裸 cargo 默认不启用推理，需显式 feature 和完整资源。

`auto` 只对 AI 输入不可用作降级，不吞前端/Rust 编译错误、签名错误或产物核对失败。开关决定在本轮构建内固定；构建失败后不得暗自再出另一种能力的同名产物。

日志明确输出“AI 已启用：模型包 … / CPU ORT …”或“AI 未启用：未配置源 / 路径不存在 / LFS未检出 / 摘要不符 …”。最终总结重复开关状态并给出本次 exe 路径；日志及可分发构建信息不写入本机绝对模型路径。

## 9. 真正无 AI 的产物

- 桌面 `photo-ai-runtime` 从默认 feature 中移出，构建计划有效时才启用；沿用 core `ai-runtime` 和现有 `cfg`，不另建推理后端。无 AI 时不编译 ort 推理实现、不分发 CPU DLL、模型和模型许可目录，也不启动/领取识别任务。
- 基础数据库、旧 AI 来源标签、禁止和纠错记录仍保留，可显示/编辑/导出。关闭推理不删旧模型设备目录，不删已有任务/标签，不改变照片整理主功能或 XMP 数据。未完成的 AI 任务留存，不自动运行。
- 前端从同一本次构建计划获得 capability，并由 Rust 编译 feature 返回的实际 capability 核对。识别/重识别按钮和命令不出现；模型设置复用原页面显示“此构建未包含 AI 识别”，不提供可点击的安装/识别入口。该状态实现前按现有 Pencil 纪律补稿，不另造设置组件。
- 调用 AI IPC 或 worker 启动参数时也要明确拒绝，而非只隐藏 UI；不得无 AI worker 时把 worker 参数误当普通 GUI 启动。
- Tauri 基础 resources 不保留无条件 `ai-*` 通配符；本次配置仅加入需要的模型/运行库路径。构建输出使用本次完整资源清单，排除上次 AI 构建留下的文件。打包、构建、bundle 使用完全相同的 feature 与资源配置。
- 已安装有 AI 版本后升级成无 AI 版本的场景列入安装器真机验收；不可仅以“新 MSI 清单不含 DLL”声称机器上没有遗留。即使磁盘残留，Rust 编译 capability 为 false 也必须拒绝推理。

构建记录加入 `ai.enabled`、包/运行库摘要与关闭原因码（不含本机路径）；debug 和 release 输出都应有身份记录。`pnpm check:win` **读待检查产物对应的构建记录和编译标记**，不重新按照当前机器的源配置猜测该 exe 是否应该有 AI。有 AI 查 worker 握手/受信包/DLL/许可；无 AI 查关闭标记/无推理资源/IPC拒绝。防止先构建 AI 再删除配置后旧 exe 被误判为合格无 AI。

`dry-run` 只读配置与输出计划，不导出、不下载、不建缓存、不写登记、不提交、不改版本。

## 10. 现有代码复用与改动落点

| 能力 | 复用/调整位置 |
| --- | --- |
| 导出与固定文本配置 | `scripts/ai/export_tinyclip.py`、`prepare_tinyclip_reference.py`、`build_tinyclip_pack.py`，收敛生产入口，保留实验工具用途 |
| 文件/包验证与原子发布 | `scripts/lib/ai-assets.mjs`；核心 `ai/pack.rs`、`ai/registry.rs`；补统一脚本层库身份与兼容解析 |
| 独立库入口 | 新增薄入口 `ai:library:init`、`ai:export`、`ai:use`；共享路径/归属/锁/指针实现 |
| 本地登记 | `ai-model-source.local.json` + 精确 Git ignore；上一轮 `model-source.json` 的未交付 HTTP 接线不作为默认模型来源 |
| 开关与运行状态 | `src-tauri/Cargo.toml`、`src-tauri/src/photo_ai.rs`、`main.rs`；已有 AI status DTO 扩展 capability，不另造状态 API |
| 前端与命令 | 现有 `AiModelSettings`、`BrowseToolbar`、`features/commands/catalog.ts`、`vite.config.ts` 的 build-info 通道；命令沿用已有 defaultKey 留空决定 |
| dev/debug/release | `scripts/debug-win.mjs`、`scripts/release.mjs`、`scripts/lib/release-windows.mjs`、`src-tauri/tauri.conf.json` 的配置装配；dev 同样通过统一决定，不仅服务安装器 |
| 构建记录/核对/镜像 | 现有 `raybend-build.json`、`scripts/check-win-artifact.mjs`、`scripts/lib/release-mirror.mjs`；沿用镜像归属和发布锁，避免第二套同步系统 |

当前 desktop 默认 feature 已清空；统一 `scripts/lib/ai-build.mjs` 决定前端 capability、原生 feature 和资源。debug、release、Tauri dev/build/bundle 使用本地源消费流程；bundle 必须与之前 build 的能力记录一致。CPU ORT 支持固定官方 ZIP 下载及校验缓存，ONNX 不在软件构建时下载或导出。模型源已登记，当前此工作区的默认 Windows 构建会启用 AI。

## 11. 验证与验收

以下是本工作单元验收清单，不是进度表：

- [x] 小型合成数据覆盖库身份/schema/空配置/空目录/中文/Unicode/大小写/长路径/链接/穿越/旧格式/字节大小上限。
- [x] 同一通用编码器供两个合成消费 profile 引用，不重复存 ONNX；profile 引用错模型/错摘要拒绝，项目选择不改写其它项目的源配置。
- [x] 完整包复用、同名损坏拒绝、失败不改旧指针、并发单写、暂存清理、复制后校验。
- [x] 临时 Git 库覆盖未提交初始分支、非根目录、dirty/index/HEAD变化、提交失败恢复、只提交本包、不产生空提交；LFS 指针/实际 ONNX 区分。
- [x] 默认目录和指定目录两条导出入口；从零 CPU 导出作为独立显式冒烟，不塞进秒级日常测试。
- [x] auto/required/off 分支；配置坏和运行库不可用分别验证降级；不得吞掉真实编译失败。
- [x] AI→无 AI→AI 连续构建，核对 feature、资源、构建记录和前端 capability；`dry-run` 无副作用；模型库输入变化不影响已冻结输入。
- [x] 无 AI core/desktop 编译、必要的单位测试和前端质量门；无 AI 不含推理依赖，已存在 AI 标签/XMP 往返测试继续通过。
- [x] 有 AI worker 真实 CPU 加载/有限向量/错误协议冒烟；无 AI AI调用明确失败、不启动GUI误处理worker参数。
- [ ] 崔总真机验收有/无 AI 两套入口表现、真实库标签与 XMP、安装器切换/干净机 CPU 运行库依赖；Agent 不代替视觉/真实库/安装器验收。

新增 CLI 是构建工具，不进入应用命令面板；已有 AI 命令要按编译 capability 控制可用性，热键继续留空并注明低频、需要明确范围。模型分发不改 XMP schema；无 AI 仍完整保留已存在 AI 来源/禁止/纠错的 sidecar 读回与写出，这一轮同时测试，不延后。

## 12. 已创建模型仓库的维护

创建名为 `model-registry` 的普通独立仓库并 clone 到与 raybend 并列的目录即可，建议仅带 README，不把现有 ONNX 手工普通 `git add`。库归属标记和 LFS 规则采用通用名称，不写 RayBend 专属归属。当前相邻库已完成归属、局部 LFS、通用编码器与 RayBend profile 初始化；远端推送和公共发布由崔总执行。

公开与否由崔总决定；本地构建不依赖公开地址。若之后需要公共 HTTP 下载，建议在模型库发布固定版本附件，建立显式源登记/校验，不自动追 `latest`，依据：[GitHub Releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases)。该扩展不会把导出步骤塞回软件打包。

实施完成证据：`implementations/2026-10-04_ai-model-registry-optional-build.md` 与 `docs/ai/tinyclip-v1/build-switch-smoke.json`。首次隔离环境依赖安装的完整联网过程未在本轮另行重跑；真实导出使用已核对的固定 CPU 环境和已缓存上游权重。GUI/真实库/安装器验收仍未勾选。
