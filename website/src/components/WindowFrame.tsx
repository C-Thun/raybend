import type { JSX } from '@solidjs/web';

/**
 * 应用窗口外框：截图统一套一层「窗口」——有标题栏、三个圆点、极细描边。
 * 平面风格（不用阴影），只为让截图看起来像软件而不是像插图。
 */
export function WindowFrame(props: { children: JSX.Element; title?: string; class?: string }) {
  return (
    <div class={['overflow-hidden rounded-2xl border border-ink/15 bg-card', props.class]}>
      <div class="flex items-center gap-2 bg-paper-2 px-4 py-2.5">
        <span class="size-2.5 rounded-full bg-ink/15" />
        <span class="size-2.5 rounded-full bg-ink/15" />
        <span class="size-2.5 rounded-full bg-ink/15" />
        <span class="ml-2 truncate text-xs text-muted">{props.title ?? 'RayBend'}</span>
      </div>
      {props.children}
    </div>
  );
}
