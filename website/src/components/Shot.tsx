import { Show } from 'solid-js';
import { IconImage } from './icons.tsx';
import { asset } from '../lib/asset.ts';
import { locale } from '../i18n/index.ts';
import { media, mediaAspect, mediaSrc, type MediaId } from '../data/media.ts';

/**
 * 站点配图。
 *
 * **图片还没到位**：`src` 为空的素材渲染成「版式正确的占位块」—— 与最终图片同宽高比、
 * 同圆角，所以真图填进 `src/data/media.ts` 之后版式不会跳。占位块上写着这张图该是什么
 * （提示词与取景说明在 `website/ASSETS.md`）。
 */

export interface ShotProps {
  id: MediaId;
  /** 无障碍描述（读屏软件用） */
  alt: string;
  class?: string;
  /** 是否懒加载（hero 的首屏图传 false） */
  eager?: boolean;
}

export function Shot(props: ShotProps) {
  const entry = () => media[props.id];
  const src = () => mediaSrc(props.id, locale());

  return (
    <Show
      when={src()}
      fallback={
        <div
          class={[
            'flex flex-col items-center justify-center gap-3 overflow-hidden rounded-2xl border border-brand/30 bg-brand-soft p-6 text-center',
            props.class,
          ]}
          // 宽高比来自素材清单（数据驱动），没有静态类名可写。
          // pi-lens-ignore: inline-styles
          style={{ 'aspect-ratio': String(mediaAspect(props.id)) }}
          role="img"
          aria-label={props.alt}
          title={entry().prompt}
        >
          <IconImage class="size-7 text-brand-dark" strokeWidth={1.75} />
          <p class="max-w-[26ch] text-sm font-medium text-ink-soft text-balance">{entry().hint}</p>
          <p class="text-xs text-muted">
            {entry().width} × {entry().height} · 待补
          </p>
        </div>
      }
    >
      {(resolved) => (
        <img
          src={asset(resolved())}
          width={entry().width}
          height={entry().height}
          alt={props.alt}
          loading={props.eager ? 'eager' : 'lazy'}
          decoding="async"
          class={['rounded-2xl', props.class]}
        />
      )}
    </Show>
  );
}
