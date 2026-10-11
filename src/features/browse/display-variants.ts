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
  /**
   * 该变体的展示尺寸 —— 直接来自 snapshot（库里存好的值，毫秒级到位）。
   * `details` 不再管它（那次全尺寸渲染只为直方图）。
   */
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
  /**
   * 选中某个显示变体后的**后台补图**（W5 §7.3）：`issue:N` 变体要保证该定稿三档齐。
   * 只通知、不等结果（失败静默，下一次选中再试）。
   */
  onVariantSelected?(repository: string, asset: number, choice: string): void;
}) {
  const [choices,setChoices]=createSignal<ReadonlyMap<number,DisplayChoice>>(new Map());
  let contextKey='',revision=0;
  const tickets=new Map<number,number>();
  return {
    choices,
    get: (asset:number)=>choices().get(asset),
    context(key:string){if(key===contextKey)return;contextKey=key;revision++;tickets.clear();setChoices(new Map());},
    /**
     * 选择一个显示变体。**snapshot（纯元数据，毫秒级）一回来就全到位** —— 网格 tile
     * 换成该变体的 imageKey、缩略图请求即刻发出（崔总 2026-09-29：tiles 只要
     * thumb，不该被重活卡住）；**尺寸也从 snapshot 直接拿**（库里存好的值），
     * 不再等 `export_variant_details` 那次全尺寸渲染（`specs/display-size-source.md` §6.1）。
     * details 退到后台只补直方图（它仍要渲染一次像素，秒级）。
     */
    async select(repository:string,asset:number,choice:string) {
      const context=revision,ticket=(tickets.get(asset)??0)+1;tickets.set(asset,ticket);
      if(choice==='latest'){setChoices(old=>{const next=new Map(old);next.delete(asset);return next;});return;}
      // 选中定稿变体：先让它三档齐（静默后台，不挡这次装载）
      deps.onVariantSelected?.(repository,asset,choice);
      const captured=await deps.snapshot(repository,asset,choice);
      if(context!==revision||tickets.get(asset)!==ticket)return;
      const natural=captured.width!==null&&captured.height!==null&&captured.width>0&&captured.height>0?{width:captured.width,height:captured.height}:undefined;
      setChoices(old=>{const next=new Map(old);next.set(asset,{choice,target:{repositoryId:repository,reference:captured.reference,captured},
        ...(natural===undefined?{}:{natural}),histogram:EMPTY_HISTOGRAM});return next;});
      /*
       * details 后台补齐：**失败不回滚**（缩略图已经能用，回滚反而把用户刚做的选择
       * 抹掉），空直方图保留为退化信号；真正的渲染失败会顺着缩略图 /
       * 大图那条路浮出来，不在这里吞成「什么都没发生过」。
       */
      void deps.details(repository,captured).then(details=>{
        if(context!==revision||tickets.get(asset)!==ticket)return;
        setChoices(old=>{const current=old.get(asset);if(!current||current.choice!==choice)return old;
          const next=new Map(old);next.set(asset,{...current,histogram:details.histogram});return next;});
      }).catch(()=>undefined);
    },
    reset(asset:number){tickets.set(asset,(tickets.get(asset)??0)+1);setChoices(old=>{if(!old.has(asset))return old;const next=new Map(old);next.delete(asset);return next;});},
    adapt(base:TilesSource):TilesSource {
      const item=(value:GridItem|null)=>{if(!value)return null;const display=choices().get(Number(value.id));
        return display?{...value,imageKey:displayVariantKey(display.target),exportVariant:display.target,aspect:display.natural?clampDisplayAspect(display.natural.width,display.natural.height):value.aspect}:value;};
      return {...base,itemAt:index=>item(base.itemAt(index)),itemById:id=>item(base.itemById(id)),
        naturalOf:id=>{const chosen=choices().get(Number(id));
          // 选了变体就**只认这个变体自己的尺寸**：还没拿到（老库 RAW 行没回填）给 null，
          // 由看图件/格子走「尺寸未知」的状态 —— 拿默认变体（= SOOC/latest）的尺寸顶上
          // 就是崔总 2026-10-11 报的「raw 缩进了 sooc 的尺寸里」。
          return chosen!==undefined?(chosen.natural??null):base.naturalOf(id);},
        aspectOf:id=>{const chosen=choices().get(Number(id));if(chosen!==undefined)return clampDisplayAspect(chosen.natural?.width??0,chosen.natural?.height??0);return base.aspectOf(id);},
        /* details 还没回来的选择仍要补基准宽高（naturalOf 的兜底读的是它） */
        ensureNatural:entries=>base.ensureNatural(entries.filter(entry=>!choices().get(Number(entry.id))?.natural))};
    },
    dispose(){revision++;tickets.clear();},
  };
}
