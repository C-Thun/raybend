/**
 * LUT 库的**形状与纯逻辑**（`design/editor.md` §3.1、`prompts/editor.pd`）。
 *
 * 只有一级分类（`.pd` 明确「只有一级分类」），每个分类下是若干 LUT 条目：
 *
 * ```text
 * LUT（面板标题）
 *  ├ 分类 A        ← 展开时其他分类自动收起；再点标题全部收起
 *  │   ├ lut-1     ← 一行两个 tile（上面预览、下面名字）
 *  │   └ lut-2
 *  └ 分类 B
 * ```
 *
 * 这里**只放数据与纯函数**（形状、清洗、建分类、展开规则）：
 * 文件读写属于 W4（`.cube` 导入），持久化属于 `lib/editor-prefs.ts`，
 * 界面在 `features/editor/lut-panel.tsx`。三处各管一段，谁也不越界。
 *
 * 放在 `lib/` 而不是 `features/` 的理由：`lib/editor-prefs.ts` 要存这份结构，
 * 而 `lib` **不许 import `features`**（`scripts/check-architecture.mjs` 的分层规则）。
 */

/** 一个 LUT 条目（一个 `.cube` 文件）。 */
export interface LutEntry {
  id: string;
  /** 显示名（默认取文件名去掉扩展名） */
  name: string;
  /** `.cube` 文件路径（W4 导入时才有） */
  path?: string;
  available?: boolean;
  coverAvailable?: boolean;
  /** 已从图库移除，但旧编辑配置仍可引用。 */
  hidden?: boolean;
}

/** 一个一级分类。 */
export interface LutCategory {
  id: string;
  name: string;
  entries: LutEntry[];
}

/** 名字长度上限（超长会把标题行撑破）。 */
export const LUT_NAME_MAX = 40;

function trimmedName(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  const name = raw.trim();
  return name === "" ? null : name.slice(0, LUT_NAME_MAX);
}

/** 任何输入 → 合法的分类表（坏条目丢弃，**不抛错**：存储里的垃圾不能让面板起不来）。 */
export function sanitizeLutCategories(raw: unknown): LutCategory[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((item) => {
    if (typeof item !== "object" || item === null) return [];
    const record = item as Record<string, unknown>;
    const id = trimmedName(record.id);
    const name = trimmedName(record.name);
    if (id === null || name === null) return [];
    const entries: LutEntry[] = Array.isArray(record.entries)
      ? record.entries.flatMap((entry) => {
          if (typeof entry !== "object" || entry === null) return [];
          const item2 = entry as Record<string, unknown>;
          const entryId = trimmedName(item2.id);
          const entryName = trimmedName(item2.name);
          if (entryId === null || entryName === null) return [];
          const path = typeof item2.path === "string" && item2.path.trim() !== "" ? item2.path : undefined;
          return [{ id: entryId, name: entryName, ...(path === undefined ? {} : { path }) }];
        })
      : [];
    return [{ id, name, entries }];
  });
}

/** 分类名是不是已经被占了（**大小写与首尾空格都算同一个**：`Travel` 与 `travel` 撞名）。
 *
 * ⚠️ 两侧都要 trim：存储里可能留着带空格的老值（`sanitizeLutCategories` 会清掉，
 * 但用户手改过 localStorage 就说不准），只 trim 入参的话就认不出它 —— 那样
 * 默认分类的保护会漏（实测：名字写成 ` default ` 时又会新建一个）。
 */
export function hasCategoryNamed(
  categories: readonly LutCategory[],
  name: string,
): boolean {
  const target = name.trim().toLocaleLowerCase();
  return categories.some(
    (category) => category.name.trim().toLocaleLowerCase() === target,
  );
}

/**
 * 「默认分类」的两个名字（中 / 英）。
 *
 * 为什么认两个：**不另设字段**（人类 2026-09-23 明确「极简处理」）——
 * 只靠名字判「已经有一个默认分类了」。于是中英切换不会造出第二个，
 * 用户把英文名改成“默认分类”也不会被当成两个。
 */
export const DEFAULT_LUT_CATEGORY_NAMES = [
  "默认分类", // i18n-exempt: 存储里的历史值（判重认名单，不是界面文案）
  "Default", // i18n-exempt: 同上（英文形态）—— 两个都要认，与当前语言无关
] as const;

/**
 * 保证分类表里**始终有一个默认分类**（幂等：已有任意一个默认名字就原样返回）。
 *
 * 口径（人类 2026-09-23）：
 *
 * * **每次启动都查一遍**（调用方在 store 创建时调）—— 用户删了也不拦，下次启动照建；
 * * 两个名字**任意一个在就不新建**；
 * * 新建出来的名字跟**当前界面语言**走（`默认分类` / `Default`），
 *    而不是写死中文 —— 英文界面下凭空出现一个中文分类很怪。
 *
 * 重名判定复用 [`hasCategoryNamed`]（大小写不敏感 + 首尾空格），于是一个小写的
 * `default` 也算数 —— 比严格对名字多一层保护，但**不会**造成任何多余的新建。
 */
export function ensureDefaultLutCategory(
  categories: readonly LutCategory[],
  defaultName: string,
  random: () => number = Math.random,
): LutCategory[] {
  const wanted = trimmedName(defaultName) ?? DEFAULT_LUT_CATEGORY_NAMES[0];
  const already = DEFAULT_LUT_CATEGORY_NAMES.some((name) =>
    hasCategoryNamed(categories, name),
  );
  if (already) return [...categories];
  const id = newLutCategoryId(categories, random);
  // 放在**最前**：它是默认落到哪个分类的目标（导入 LUT 时的默认项）
  return [{ id, name: wanted, entries: [] }, ...categories];
}

/** 建一个分类 id（本地唯一即可；不引第三方 uuid）。 */
export function newLutCategoryId(
  categories: readonly LutCategory[],
  random: () => number = Math.random,
): string {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const id = `lutcat-${Math.floor(random() * 0xffffffff).toString(36)}`;
    if (!categories.some((category) => category.id === id)) return id;
  }
  // 极端情况下（随机源被固定住）退化成按序号 —— 一定会终止
  return `lutcat-${categories.length + 1}`;
}

export interface AddCategoryResult {
  categories: LutCategory[];
  /** 新分类的 id；重名时为 `null`（**不改原数组**） */
  createdId: string | null;
}

/** 新建分类（重名不建、不改原数组 —— 调用方据此提示）。 */
export function addLutCategory(
  categories: readonly LutCategory[],
  rawName: string,
  random: () => number = Math.random,
): AddCategoryResult {
  const name = trimmedName(rawName);
  if (name === null || hasCategoryNamed(categories, name)) {
    return { categories: [...categories], createdId: null };
  }
  const id = newLutCategoryId(categories, random);
  return { categories: [...categories, { id, name, entries: [] }], createdId: id };
}

/**
 * 展开 / 收起某个分类。
 *
 * 规则（`.pd`）：**同一时刻只展开一个**；点已经展开的那个 = 全部收起（返回 `null`）。
 */
export function toggleExpandedCategory(
  current: string | null,
  id: string,
): string | null {
  return current === id ? null : id;
}

/** 分类里的 LUT 数量（界面上跟在分类名后面）。 */
export function visibleLutCategories(categories: readonly LutCategory[]): LutCategory[] {
  return categories.map((category) => ({ ...category, entries: category.entries.filter((entry) => !entry.hidden) }));
}

export function lutCount(categories: readonly LutCategory[]): number {
  return categories.reduce((total, category) => total + category.entries.filter((entry) => !entry.hidden).length, 0);
}
