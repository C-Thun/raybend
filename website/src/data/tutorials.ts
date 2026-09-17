/**
 * B 站视频教程。
 *
 * **现在是空的**（用户还在录）—— 页面据此渲染「教程录制中 · 即将上线」占位块。
 * 录完往数组里加条目即可，版式不用改：
 *
 * ```ts
 * { title: '导入与建库（10 分钟）', url: 'https://www.bilibili.com/video/BVxxxxxxxxxx' }
 * ```
 */
export interface Tutorial {
  /** 视频标题（用户自己写，不强制双语） */
  title: string;
  /** B 站视频地址 */
  url: string;
}

export const tutorials: readonly Tutorial[] = [];
