import type { Dictionary } from '../i18n/zh.ts';
import type { MediaId } from './media.ts';

/**
 * 站点的**结构**数据：只放 id、图标、版式与图片引用 —— 文案一律在 `src/i18n/`。
 * 加一个阶段 / 一条特性 = 往数组里加一项（文案键同步加两个语言），版式代码不用改。
 */

export type WorkflowId = keyof Dictionary['workflows']['items'];
export type HighlightId = keyof Dictionary['highlights']['items'];
export type FeatureId = keyof Dictionary['features']['items'];

/** 工作流总览的四个阶段（顺序即叙事顺序） */
export const workflowStages: readonly WorkflowId[] = ['import', 'browse', 'organize', 'export'];

export const highlights: readonly HighlightId[] = ['local', 'fast', 'open'];

export interface FeatureRow {
  id: FeatureId;
  /** 配图（还没有真图时渲染同尺寸占位块） */
  media: MediaId;
  /**
   * 版式：`text-first` = 左文右图；`media-first` = 左图右文；
   * `band` = 整宽带（文字在左、装饰在右）
   */
  layout: 'text-first' | 'media-first' | 'band';
}

export const featureRows: readonly FeatureRow[] = [
  { id: 'import', media: 'featureImport', layout: 'text-first' },
  { id: 'browse', media: 'featureBrowse', layout: 'media-first' },
  { id: 'keyboard', media: 'featureKeyboard', layout: 'band' },
];
