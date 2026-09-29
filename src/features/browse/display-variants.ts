/** Component-local draft choices + one data adapter over the shared browse source. */
import { createSignal } from 'solid-js';
import type { VariantSnapshot } from '../../lib/export-model.ts';
import { displayVariantKey, type DisplayVariant } from '../../lib/display-variant.ts';
import { clampDisplayAspect } from '../../lib/tile-flow.ts';
import type { TilesSource, GridItem } from '../../components/ui/tiles/source.ts';
import type { HistogramCounts } from '../../lib/histogram.ts';
export interface DisplayChoice {
  choice: string;
  target: DisplayVariant;
  /** details（要一次全尺寸渲染）还没回来时缺省：aspect / natural 退回基准照片。 */
  natural?: {width:number;height:number};
  /** details 没回来时是**空直方图**（bins 0）：右栏画空态，绝不回退成基准照片的直方图。 */
  histogram: HistogramCounts;
}
/**
 * 「还没渲染出来」的直方图：0 柱 ⇒ `histogramBarHeights` 给空条，界面画空态。
 * 传 `undefined` 不行 —— `HistogramPanel.countsOverride !== undefined` 会落回
 * 按路径取的**基准照片**直方图，切了变体却显示原图的数据，比空着更骗人。
 */
const EMPTY_HISTOGRAM: HistogramCounts = { bins: 0, r: [], g: [], b: [], max: 0 };

export function createBrowseDisplay(deps: {
  snapshot(repository: string, asset: number, choice: string): Promise<VariantSnapshot>;
  details(repository: string, captured: VariantSnapshot): Promise<{width:number;height:number;histogram:HistogramCounts}>;
}) {
  const [choices,setChoices]=createSignal<ReadonlyMap<number,DisplayChoice>>(new Map());
  let contextKey='',revision=0;
  const tickets=new Map<number,number>();
  return {
    choices,
    get: (asset:number)=>choices().get(asset),
    context(key:string){if(key===contextKey)return;contextKey=key;revision++;tickets.clear();setChoices(new Map());},
    /**
     * 选择一个显示变体。**snapshot（纯元数据，毫秒级）一回来就挂上** —— 网格 tile
     * 立刻换成该变体的 imageKey、缩略图请求即刻发出（崔总 2026-09-29：tiles 只要
     * thumb，不该被重活卡住）；宽高与直方图要 `export_variant_details` 做一次
     * **全尺寸渲染**（秒级），那部分退到后台补齐（`natural` / `histogram` 后来更新）。
     * view / film 的大图走自己的 'screen' 装载路径，不依赖 details。
     */
    async select(repository:string,asset:number,choice:string) {
      const context=revision,ticket=(tickets.get(asset)??0)+1;tickets.set(asset,ticket);
      if(choice==='latest'){setChoices(old=>{const next=new Map(old);next.delete(asset);return next;});return;}
      const captured=await deps.snapshot(repository,asset,choice);
      if(context!==revision||tickets.get(asset)!==ticket)return;
      setChoices(old=>{const next=new Map(old);next.set(asset,{choice,target:{repositoryId:repository,reference:captured.reference,captured},histogram:EMPTY_HISTOGRAM});return next;});
      /*
       * details 后台补齐：**失败不回滚**（缩略图已经能用，回滚反而把用户刚做的选择
       * 抹掉），空直方图与基准 aspect 保留为退化信号；真正的渲染失败会顺着缩略图 /
       * 大图那条路浮出来，不在这里吞成「什么都没发生过」。
       */
      void deps.details(repository,captured).then(details=>{
        if(context!==revision||tickets.get(asset)!==ticket)return;
        setChoices(old=>{const current=old.get(asset);if(!current||current.choice!==choice)return old;
          const next=new Map(old);next.set(asset,{...current,natural:{width:details.width,height:details.height},histogram:details.histogram});return next;});
      }).catch(()=>undefined);
    },
    reset(asset:number){tickets.set(asset,(tickets.get(asset)??0)+1);setChoices(old=>{if(!old.has(asset))return old;const next=new Map(old);next.delete(asset);return next;});},
    adapt(base:TilesSource):TilesSource {
      const item=(value:GridItem|null)=>{if(!value)return null;const display=choices().get(Number(value.id));
        return display?{...value,imageKey:displayVariantKey(display.target),exportVariant:display.target,aspect:display.natural?clampDisplayAspect(display.natural.width,display.natural.height):value.aspect}:value;};
      return {...base,itemAt:index=>item(base.itemAt(index)),itemById:id=>item(base.itemById(id)),
        naturalOf:id=>choices().get(Number(id))?.natural??base.naturalOf(id),
        aspectOf:id=>{const natural=choices().get(Number(id))?.natural;return natural?clampDisplayAspect(natural.width,natural.height):base.aspectOf(id);},
        /* details 还没回来的选择仍要补基准宽高（naturalOf 的兜底读的是它） */
        ensureNatural:entries=>base.ensureNatural(entries.filter(entry=>!choices().get(Number(entry.id))?.natural))};
    },
    dispose(){revision++;tickets.clear();},
  };
}
