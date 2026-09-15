/**
 * 工作流模型与 toolsbar 装配表（`ARCHITECTURE.md` §1：`src/shell/` 只做布局与装配）。
 *
 * 为什么把「工作流有哪些」「某个工作流下 toolsbar 放什么」抽成纯数据：
 *   1. 它会被三处用到（titlebar 无、flowbar 的分段控件、toolsbar 的显隐），
 *      散在 TSX 里必然各处各写一份
 *   2. 它是**可测的**：显隐规则错一个，界面上是「整行不见了」这种很难一眼看出的 bug
 *
 * 设计依据（`design/main.md` §2.2）：四个工作流是**有序流水线**，不是并列选项 ——
 * 顺序固定为 导入 → 浏览 → 编辑 → 导出，不因「最近使用」重排。
 */

import type { MessageKey } from "../i18n/index.ts";

/** 工作流的固定顺序（**不要**重排；顺序本身就是语义） */
export const WORKFLOWS = ["import", "browse", "edit", "export"] as const;

export type WorkflowId = (typeof WORKFLOWS)[number];

/** 默认落在「导入」（M1 只有这一段有内容） */
export const DEFAULT_WORKFLOW: WorkflowId = "import";

/** 工作流 → 文案 key */
export const WORKFLOW_LABEL_KEY: Record<WorkflowId, MessageKey> = {
 import: "flow.import",
 browse: "flow.browse",
 edit: "flow.edit",
 export: "flow.export",
};

export function isWorkflow(value: unknown): value is WorkflowId {
 return (
  typeof value === "string" && (WORKFLOWS as readonly string[]).includes(value)
 );
}

/** 非法值一律回落到默认工作流（存储里可能是旧版本写的） */
export function normalizeWorkflow(value: unknown): WorkflowId {
 return isWorkflow(value) ? value : DEFAULT_WORKFLOW;
}

/** 工作流在流水线里的位置（0 起）。已知 id 之外的输入返回 -1 */
export function workflowIndex(value: unknown): number {
 return isWorkflow(value) ? WORKFLOWS.indexOf(value) : -1;
}

/* ══════════════════════════════════════════════════════════════
 * toolsbar 装配
 * ══════════════════════════════════════════════════════════════ */

/**
 * 工具按钮的标识。**只有 id**，图标与点击行为留在视图层（`ToolsBar.tsx`），
 * 这样这个文件不依赖任何组件，纯数据可测。
 */
export type ToolId = "batch-exclude";

export interface ToolSpec {
 id: ToolId;
 labelKey: MessageKey;
 /**
  * 是否在「没有选中项」时禁用。
  * `批量排除` 是**反转**操作（`DESIGN.md` §12.2）：没有选中项时无从反转，必须禁用。
  */
 disabledWhenEmpty?: boolean;
}

const TOOL_CATALOG: Record<ToolId, ToolSpec> = {
 "batch-exclude": {
  id: "batch-exclude",
  labelKey: "tools.batch_exclude",
  disabledWhenEmpty: true,
 },
};

/** 每个工作流下 toolsbar 有哪些工具。空数组 → 整行隐藏（`design/main.md` §2.3） */
const TOOLS_BY_WORKFLOW: Record<WorkflowId, readonly ToolId[]> = {
 import: ["batch-exclude"],
 browse: [],
 edit: [],
 export: [],
};

/** 该工作流的工具清单（按声明顺序） */
export function toolsFor(workflow: WorkflowId): readonly ToolSpec[] {
 const ids = TOOLS_BY_WORKFLOW[normalizeWorkflow(workflow)] ?? [];
 return ids.map((id) => TOOL_CATALOG[id]);
}

/**
 * 工具此刻是否该禁用。
 *
 * `hasSelection` 由上层（工作区）传入 —— shell 不知道照片选择的状态，
 * 那是 `photo-grid` 模块的事（`ARCHITECTURE.md` §3 的状态归属）。
 */
export function isToolDisabled(spec: ToolSpec, hasSelection: boolean): boolean {
 return Boolean(spec.disabledWhenEmpty) && !hasSelection;
}
