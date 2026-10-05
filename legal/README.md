# 许可原文与来源

许可证属于各自权利人的许可文本；这些副本用于随应用分发，**不会因为存放在 AGPL 仓库里而变更许可**。

`upstream-license-overrides.json` 保存 Cargo 发布包遗漏的许可证原文及其上游精确 commit URL。生成器优先使用包内文件，并补上这些原文。没有原文时只允许已保存的标准 SPDX 文本补足，同时保留包元数据作者并标注 `spdx-fallback`；不发明原始版权年份/持有人。

- `MIT.txt`、`MPL-2.0.txt`、`CC-BY-SA-3.0.txt`：SPDX license-list-data v3.28.0 的标准原文，来源 https://github.com/spdx/license-list-data/tree/v3.28.0/text 。标准 MIT 中的模板版权占位符不是已确认的组件原始版权声明；组件作者来源于其发布包。
- `dav1d-1.5.0.txt`：VideoLAN dav1d 1.5.0 COPYING，来源 https://github.com/videolan/dav1d/blob/1.5.0/COPYING 。Windows 使用这一版静态库，不用 WSL 1.4.1 系统库冒充。
- LensFun 校准数据署名 LensFun community，CC-BY-SA-3.0，来源 https://github.com/lensfun/lensfun ，由 lensfun 0.7.0 内嵌数据库使用。
- libwebp 的 vendor/COPYING/PATENTS 随 libwebp-sys 收录，绑定声明 MIT 的文本另行补足，不能把 BSD 的 C 库声明当作 MIT 绑定许可。

目前 SPDX 补足组件为 libwebp-sys 0.9.6、selectors 0.36.1、simd_helpers 0.1.0；公开发布审核需核对它们包内作者/文件头与上游许可声明。生成器不会把“有 SPDX 标识”宣称成“已获得另行授权”。

`public/legal/third-party.json` 是可重生成的分发资源，含依赖锁文件摘要；应用本体与官网的清单分开。release 会自动刷新，独立刷新用 `pnpm licenses:generate`。

## 2026-09-27 补足复核

已读取三个锁定包的 Cargo 声明、作者、文件头和 `.cargo_vcs_info.json`，并查询精确 commit 的上游文件树：libwebp-sys `4007a323c1dcc4ad11d70ddadffc51ecfa1dbb5e`、selectors `635e1a19d02960588a00e189bd4bd5bdb150ec3d`、simd_helpers `ca1a2f84aa386d758e98f8a609d990263932fb85`。

selectors 的 lib.rs/parser.rs 明确引用 MPL-2.0；所附标准 MPL 原文与声明相符，作者 The Servo Project Developers 已保留。libwebp-sys 的绑定声明 MIT、vendor 库另附 COPYING/PATENTS，simd_helpers 声明 MIT；二者精确源码树均未提供独立 MIT 版权原文，包内也未找到相应文件头。没有把模板 `<year> <copyright holders>` 改成猜测署名或把 BSD C 库的声明当作绑定 MIT。两项 MIT 原始版权补足仍需公开发行前确认；生成器保留 spdx-fallback 和作者，不冒称原文已取得。

## ONNX Runtime CPU 来源（2026-10-04）

`onnxruntime-1.28.0/LICENSE` 与 `ThirdPartyNotices.txt` 原样取自微软固定 `onnxruntime-win-x64-1.28.0.zip`，来源 https://github.com/microsoft/onnxruntime/releases/tag/v1.28.0 。制品 78,796,801 bytes，SHA-256 `abef733dacbe2f571547a7150b479b5cb9cc0df22f96c24983a42cadb1b4f8bc`。CPU DLL 与这两份原文已接入默认应用私有资源；Windows debug 资源核对与完整桌面 CPU worker 冒烟通过。正式安装器尚未生成或验收，不以 MIT 一行替代上游第三方声明。

TinyCLIP 已定为首版：元数据与许可入仓，经过摘要校验的大权重为忽略 Git 的构建输入，随应用资源提供；SigLIP 2 仅留未来考查与历史实验，不进默认产物。模型包随附权重与导出来源的许可声明，不能以运行库的 MIT 代替权重许可。

## 色彩依赖原文（2026-10-04）

lcms2 6.2.0 的 LICENSE 署名 Kornel Lesiński；lcms2-sys 4.0.7 的 vendor/LICENSE 署名 Marti Maria Saguer，vendor 头部 LCMS_VERSION=2190（2.19.0）。Windows 已静态链接。现有许可生成器直接收录绑定与 vendor 原文，不用另造许可文本，也不要求用户安装系统 DLL；内置 ICC 是本项目生成的固定配置，说明在 crates/raybend/assets/color/README.md。

## TinyCLIP 首版与 CPU 运行库分发（2026-10-04）

崔总已指定TinyCLIP作为本期默认方案。`tinyclip-95ec8197/MODEL-CARD.md` 是固定权重revision的原始模型卡，声明MIT；`LICENSE` 是上游Cream仓库MIT原文，保留Microsoft版权声明，不虚构独立模型版权年份/作者。`crates/raybend/assets/ai/tinyclip-v1/LICENSE.txt` 随模型包保留来源与原文。FP32图像编码器仅由固定权重导出，文字特征预计算，未训练新权重。

`crates/raybend/assets/ai/ort-win-x64/` 保存固定运行库元数据及两份原文，CPU DLL作为经过SHA校验的生成资源随应用分发；不包含PDB、头文件或GPU运行库。大权重/DLL忽略Git，由构建资源导入脚本校验；不由Agent发布任何制品。
