# issue 的 XMP 互操作契约（M3-W7）

此文档定义未来 sidecar 读写时的数据表示；M3 的真相源仍是 `catalog.db`，**当前版本尚未实现 XMP 写入或导入**。原始照片文件始终不写回。

## 语义

- `SOOC` 和 `RAW` 是从文件推导的特殊源，不序列化为用户定稿。`latest` 是一份自动保存的可变工作副本；切换 issue 或撤销/重做也会更新它。
- 用户定稿是不可变完整 profile，名称、编辑基准（`raw` / `sooc`）、创建时间和 `schema_version` 均保存；同一照片可以有多份。定稿中的 LUT 以应用库稳定 ID 引用，不把外部 LUT 文件嵌入 XMP。缺失 LUT 时保留引用并提示，不能静默改变 profile。
- 选中态不写入 XMP 或 DB：用当前 latest 的规范 profile 哈希加完整配置比较推导。哈希仅用于快速筛选，发生碰撞时以完整配置比较为准。

## 表达

使用自有命名空间 `https://raybend.app/ns/issue/1.0/`（前缀示例 `rb`）。XMP RDF 中 `rb:profiles` 是有序 `rdf:Seq`：每个 `rdf:li` 是资源，含 `rb:name`（UTF-8）、`rb:sourceBase`、`rb:createdAtMs`、`rb:schemaVersion`、`rb:profileHash`、`rb:profileJson`。`rb:profileJson` 是 `DevelopStack` 当前版本的规范 JSON，写入 XML 时做实体转义；字段顺序由有序 map 保证。`rb:latest` 使用相同结构表达工作副本，另设 `rb:latestSchemaVersion`，不复用定稿 ID。数据库自增 ID 仅在本库有效，不写进 sidecar 作为跨库身份。

`rb:profileHash` 为当前实现的 64 位 FNV-1a 十六进制；导入时重新计算并验证完整 JSON。未知 `schemaVersion` 必须保留原 XMP 并报不支持，不能按旧版本硬解析。未来修改 profile 结构时升版本并编写明确迁移。

## 同步边界

XMP 是互操作副本，`catalog.db` 为本地真相源。M3 不自动导入或覆盖 sidecar。未来接入时，导入同一照片的同哈希定稿需再比对完整 profile；若名称或创建时间冲突，保留数据库定稿并显式报告，不自动覆盖。写出采用临时文件加原子替换，失败不回滚已经落库的编辑；重试由同步任务负责。
