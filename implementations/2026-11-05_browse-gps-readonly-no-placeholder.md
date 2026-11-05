# 浏览右栏 GPS 行：只读就别装成能编辑（无坐标整行不显示）

> 完成时间：**2026-11-05 02:12:40 +08:00**
> 起因：崔总实测 —— 右栏「地理信息 → GPS 经纬」显示 `未填写（点这里编辑）`，**点下去完全没反应**。
> 结论：GPS 是**只读**字段（只从文件 EXIF 来，产品不提供手填），不该复用可编辑字段那句占位。

## 1. 设计口径（回答「这块是怎么设计的」）

| 项 | 事实 | 代码位置 |
| --- | --- | --- |
| 数据来源 | 导入时从原图 EXIF 读出经纬写进 `catalog.assets.gps_lat/gps_lon`，**之后不再变**；右栏不提供手填 | `crates/raybend/src/import/`、`store/query.rs` |
| 展示 | 十进制 6 位（`EasyCopy` 可复制）+ 「在地图中打开」（OpenStreetMap） | `src/features/browse/BrowsePanels.tsx::GpsField` |
| 可编辑的是哪几项 | 只有它下面那四项人写的：Country / Province-State / City / Sublocation（带建议列表） | 同文件 `EditableField` |
| 联动 | 有坐标、且上面三项有空缺时出现「按 GPS 填入地点」（离线城市表逆地理编码，只填空字段、一步撤销） | `geo.rs` + `store/marking.rs::fill_geo` |
| 规格来源 | `specs/browse-info-fields.md`（2026-11-03 那张表第 4 项：GPS 只读 + 复制 + 地图） | — |

**错误在占位文案**：`GpsField` 复用了 `t("browse.fieldEmpty")` =「未填写（点这里编辑）」——
那是给可编辑字段的（作者/描述/国家/省/市/具体地点）。GPS 没有编辑态，于是「叫你点」而「点了没用」。

## 2. 改法（崔总拍板：有信息就显示，没信息就不显示）

- `GpsField` 去掉 `placeholder` 入参；外层改为 `<Show when={text()}>`，**坐标缺失/非有限/越界时整行不渲染**
  （标签、坐标、地图入口一起消失）。
- 其余四项的「未填写（点这里编辑）」原样保留 —— 它们真有编辑态。
- 顺带的好处：地理段不再因为「GPS 一行空占位」而多出一行高度；相机没 GPS / 导入没读到是常态。

## 3. 涉及文件

- `src/features/browse/BrowsePanels.tsx`（`GpsField` 签名 + 渲染；唯一调用点去掉 `placeholder`）
- `specs/browse-info-fields.md` §3.5、`design/browse.md` §2.4、`memory/FUNCTION-BROWSE.md` §6（三处同步「无坐标整行不显示」）

## 4. 验证

- `pnpm typecheck`：通过。
- `pnpm test`：**1415 passed / 0 failed**。
- `pnpm lint:colors` / `pnpm lint:arch` / `pnpm lint:i18n`：全部通过（未新增文案，语言包无需改）。
- **未做**：真机 E2E（右栏在有/无 GPS 两张照片间的切换观感）与 `design/browse.pen` 补帧，属人工项。

> 未加单测的原因：这是纯渲染分支（`<Show>` 有无内容），仓库没有组件渲染测试设施
> （`src/**/*.test.tsx` 为零），经纬度合法性与之前一致、未改动。

## 5. 遗留

1. 若将来产品要做**手填/改 GPS**（写回 XMP `exif:GPS*` 或只存库），那要单独立规格：
   涉及 `specs/xmp-sidecar.md` 映射、反向地理编码语义、撤销——不在本次范围。
2. 编辑器左栏的 GPS 行走 `MetadataRows`，空值本来就不渲染，与本次口径一致，无需改。
