/**
 * import / browse 共用的完整中列照片舞台：
 * `PhotoGrid + Viewer/CompareView + FilmStrip + statusbar`。
 *
 * 最重要的结构保证：`PhotoGrid` 永远挂载，只在看图时 `invisible`。
 * 因此进入 view / film / compare 不会销毁虚拟列表、滚动位置或键盘焦点接续逻辑。
 */

import { Show, type JSX } from "solid-js";

import type { ThumbQueue } from "../../components/ui/thumb-queue.ts";
import {
  PhotoStatusBar,
  TilesShell,
  type TilesControlBarProps,
  type TilesViewingInfo,
} from "../../components/ui/tiles/index.ts";
import {
  CompareView,
  FilmStrip,
  Viewer,
} from "../../components/ui/viewer/index.ts";
import type { TilesSource } from "../../components/ui/tiles/source.ts";
import { PhotoGrid, type PhotoGridProps } from "./PhotoGrid.tsx";
import type { PhotoViewingController } from "./viewing.ts";

export interface PhotoViewingStageProps {
  source: TilesSource;
  controller: PhotoViewingController;
  thumbs: ThumbQueue;
  filmStripStep: number;
  onFilmStripStepChange: (step: number) => void;
  tilesBar: TilesControlBarProps;
  viewingInfo: TilesViewingInfo;
  pinsKey?: string;
  focusId?: string;
  movesWithArrowKeys?: boolean;
  onInteract?: () => void;
  onFocusIndex?: (index: number) => void;
  watermark?: PhotoGridProps["watermark"];
  class?: string;
}

export function PhotoViewingStage(props: PhotoViewingStageProps): JSX.Element {
  const active = (): boolean => props.controller.viewer.state().active;
  const gridClass = (): string =>
    [active() ? "invisible pointer-events-none" : "", props.class ?? ""]
      .filter(Boolean)
      .join(" ");

  return (
    <>
      <TilesShell bar={active() ? null : props.tilesBar}>
        <PhotoGrid
          source={props.source}
          viewer={props.controller.viewer}
          openRequest={props.controller.openRequest()}
          onOpeningViewer={props.controller.prepareViewer}
          class={gridClass()}
          {...(props.pinsKey === undefined ? {} : { pinsKey: props.pinsKey })}
          {...(props.focusId === undefined ? {} : { focusId: props.focusId })}
          {...(props.movesWithArrowKeys === undefined
            ? {}
            : { movesWithArrowKeys: props.movesWithArrowKeys })}
          {...(props.onInteract === undefined ? {} : { onInteract: props.onInteract })}
          {...(props.onFocusIndex === undefined
            ? {}
            : { onFocusIndex: props.onFocusIndex })}
          {...(props.watermark === undefined ? {} : { watermark: props.watermark })}
        />

        <Show when={active()}>
          <Show
            when={props.controller.comparing()}
            fallback={
              <Viewer
                store={props.controller.viewer}
                class="z-10"
                onClose={props.controller.resetChrome}
              />
            }
          >
            <CompareView
              photos={props.controller.comparePhotos()}
              selectedCount={props.controller.selectedCount()}
              store={props.controller.viewer}
              onClose={props.controller.resetChrome}
              onFocus={props.controller.focusComparePhoto}
              class="z-10"
            />
          </Show>
        </Show>
      </TilesShell>

      <Show when={active() && props.controller.filmVisible()}>
        <FilmStrip
          viewer={props.controller.viewer}
          selectedIds={props.source.selection().ids}
          onSelect={(id, mode) => props.source.select(id, mode)}
          thumbs={props.thumbs}
          onlyIds={props.controller.filmOnlyIds()}
          sizeStep={props.filmStripStep}
          onSizeStepChange={props.onFilmStripStepChange}
          class="shrink-0"
        />
      </Show>

      <Show when={active()}>
        <PhotoStatusBar info={props.viewingInfo} />
      </Show>
    </>
  );
}
