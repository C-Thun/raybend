# 快捷键场景化（Command Scenes）— 工作单元规格

**状态**：已定案待实施（崔总 2026-10-10）。
**关联**：`src/lib/commands.ts`（冲突判定）、`src/features/commands/catalog.ts`（96 条命令）、
`ShortcutSettingsDialog.tsx`（快捷键设置）、`CommandPalette.tsx`（Ctrl+K）；
`specs/M2-W3.md`（命令体系原始规格，本单元修订其 §2.4 冲突判据）；
`specs/library-recycle-bin.md`（并行单元，依赖本单元解决 browse/export 共用 `Delete` 键的问题）。

---

## 0. 背景与决定（崔总 2026-10-10）

界面操作 > 快捷键 > 菜单。同一条键在不同场景下承担不同功能是**正常需求**
（现状就有：`0`–`5` 在网格打星、在看图缩放）。但现在的冲突检测只认「作用域」
（`global/tiles/viewer`），导致 `Delete` 这类跨场景复用的键被判成真冲突、命令被迫合成一条
动态命令（`edit.delete` 按 flow 切换标题与行为）。

**决定**：

1. 引入**场景**维度：场景 = 顶部 flow，**只 4 个**（导入 / 浏览 / 编辑 / 导出）。
2. 冲突判定改为 **场景相交 ∧ 作用域相交**；跨场景同键 = 合法（UI 说明，不再叫冲突）。
3. `Ctrl+K` 命令面板与快捷键设置界面**加一列场景说明**，让用户理解「同键可以在不同场景共存」。
4. 处理完把 `edit.delete` 拆成两条（browse 删除 / export 移出队列），各自绑 `Delete` 并存。
5. 随后 **review 现有全部命令**：补场景标注、检查默认键位，做一次整理（清单见 §4）。

---

## 1. 数据模型

### 1.1 场景字段

```ts
export type CommandScene = "import" | "browse" | "edit" | "export";
export interface CommandSpec {
  /** 这条命令在哪些场景里成立（静态声明；运行时仍由 when() 精确判定） */
  scenes: readonly CommandScene[];   // 必填；全场景用 ALL_SCENES 常量
  // 其余字段不变
}
```

- **有默认键位的命令必填**（`spec()` 当场 throw，`catalog.test.ts`/`scenes.test.ts` 盯着）；
  无键命令由 `spec()` **按 id 前缀推断**（`editor.*`→edit、`export.*`→export、`import.*`→import、
  `browse./recycle./ai./organization./nav./mark.`→browse、其余→全场景），显式给的优先。
  理由（实施后修订）：冲突检测的正确性只依赖**有键**命令 —— 96 条逐条手标收益低、漂移风险高；
  推断规则集中在 `spec()` 一处、可测（`scenes.test.ts` 对真实注册表体检）。
- `scenes` **只用于**：① 冲突检测；② 面板/设置的场景展示与搜索。
  **运行时执行判定仍只看 `when()` / `enabled()`**（`availabilityOf` 语义不变——判定只有一份的纪律不许破）。
- 一致性纪律：`scenes` 必须与 `when()` 的场景范围一致（实施 review 时逐条核对，§4）。

### 1.2 冲突判定（`lib/commands.ts::detectConflicts`）

```text
冲突（blocking）  ⇔  存在一对命令：scenesOverlap(a,b) ∧ scopeOverlaps(a.scope,b.scope)
共享（提示）      ⇔  同键命令之间场景都不相交
```

- `scenesOverlap` = 两个场景数组交集非空。
- `BindingIssue.kind` 保留 `duplicate | shared | reserved | risky | invalid`；`shared` 的
  文案改为中性说明（「同一键位在不同场景」），仍为非阻断。
- 现有「同 scope 同键」判定全部复核一遍（`0`–`5` 的 tiles/viewer 组、`Esc`、`Enter` 等）。

### 1.3 存量兼容

- 覆盖表（`shortcuts.ts` 存储）按**命令 id** 保存，机制不变。
- `edit.delete` 拆分后旧 id 的覆盖**不做迁移**（项目未发布；仅记录）。导入导出文件格式不变。
- `CommandSpec` 新增必填字段 → `catalog.test.ts` 与冒烟替身同步更新。

---

## 2. 界面

### 2.1 命令面板（`CommandPalette`）

- 每行在键位右侧/标题区加入**场景标签**（中文：导入 / 浏览 / 编辑 / 导出；多场景并列显示全部，
  四场景都有显示「通用」）。形态与现有「当前场景不可用」提示共存，不抢版面（设计稿定）。
- 场景词进入搜索索引（搜「导出」可筛出导出场景的命令）——与「键位本身也进搜索索引」同一口径。
- 不可用原因展示不变（`when` / `enabled` 两级）。

### 2.2 快捷键设置（`ShortcutSettingsDialog`）

- 表格加**场景列**（同上标签）；顶部一句说明（**带变量或概念解释**，合规 §2.20：
  「不同场景可以使用相同快捷键，冲突只在同一场景内检查」——这是概念说明，按 §2.20 应进 hover 提示
  或界面常驻？**定案：常驻一行**，因为它是操作说明而非重复注解；具体形态设计稿定）。
- 冲突展示：`duplicate`（同场景真冲突）仍拦保存；`shared` 显示为中性提示。
- 危险键提示（`risky`）不变。

### 2.3 其他消费方

- 标题栏菜单：不展示场景（菜单项按现有 `when` 显示/禁用，不变）。
- 快捷键冲突的**分发器**：不改（`when` 已保证运行时正确性）。

---

## 3. 命令拆分（`edit.delete`）

| 现状 | 拆为 | titleKey | scope | scenes | defaultKey | run |
| --- | --- | --- | --- | --- | --- | --- |
| `edit.delete`（动态切） | `browse.delete` | 新增 `cmd.browse.delete`（文案沿用「删除」） | `tiles` | `["browse"]` | `Delete` | `deps.browse.requestDelete()` |
| 同上 | `export.remove` | 沿用 `cmd.export.remove`（「将选中待处理定稿移出队列」） | `tiles` | `["export"]` | `Delete` | `deps.export.remove()` |

- 两条命令面板文案各自独立（不再动态切换）。
- **回收站单元依赖**：`browse.delete` 的 `when` 需排除回收站视图（见回收站 spec §6）。
- 其余「动态 titleKey/run」的命令一并检查（如有同类，同样拆分或明确保留理由）。

---

## 4. 现有快捷键 review 清单

实施时逐条过 `catalog.ts`（96 条），产出在实施记录里；本 spec 给规则：

1. **逐条标注 `scenes`**，并与 `when()` 交叉核对；
2. **默认键位体检**（按场景分组列出所有键位，检查是否有该给没给的、或该收的）：
   - flow 切换 `Mod+1..4`：全场景；
   - 补上明显缺失的高频键（如有）——**只补不reshuffle**，改键要有理由；
3. 键位冲突矩阵输出（同键命令的「场景 × 作用域」表格）进实施记录，作为审查证据；
4. `pnpm test` 中 `commands.test.ts` / `catalog.test.ts` / `palette.test.ts` 同步更新。

---

## 5. 测试

- `commands.test.ts`：跨场景同键不冲突（browse 删除 vs export 移出队列）；同场景同键冲突（对照组）；
  场景 × 作用域四象限（含 `global` 与具体场景相交）；`shared` 提示的判定。
- `catalog.test.ts`：每条命令都有非空 `scenes`；拆分的两条命令 id/文案/键位正确。
- `ShortcutSettingsDialog` / `CommandPalette`：场景标签渲染（存在、多场景、通用）、搜索命中场景词。
- 冒烟：`pnpm typecheck && pnpm test && pnpm lint:* && pnpm build`；`Ctrl+K` 与设置面板的目视归崔总。

---

## 6. 验收清单

- [ ] `CommandSpec.scenes` 落地：**有键命令全部显式标注**；无键命令按前缀推断（规则在 `spec()` 一处，有体检测试）
- [ ] 冲突判定 = 场景相交 ∧ 作用域相交；跨场景同键放行
- [ ] `browse.delete` 与 `export.remove` 拆分完成，`Delete` 键在浏览与导出各自生效且互不冲突
- [ ] `Ctrl+K` 面板显示场景列；场景词可搜索
- [ ] 快捷键设置显示场景列与一句说明；真冲突仍拦保存
- [ ] 现有键位 review 完成，实施记录含冲突矩阵与变更理由
- [ ] 单测与冒烟通过
