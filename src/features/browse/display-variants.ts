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
  natural: {width:number;height:number};
  histogram: HistogramCounts;
}
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
    async select(repository:string,asset:number,choice:string) {
      const context=revision,ticket=(tickets.get(asset)??0)+1;tickets.set(asset,ticket);
      if(choice==='latest'){setChoices(old=>{const next=new Map(old);next.delete(asset);return next;});return;}
      const captured=await deps.snapshot(repository,asset,choice);
      const details=await deps.details(repository,captured);
      if(context!==revision||tickets.get(asset)!==ticket)return;
      setChoices(old=>new Map(old).set(asset,{choice,target:{repositoryId:repository,reference:captured.reference,captured},natural:{width:details.width,height:details.height},histogram:details.histogram}));
    },
    reset(asset:number){tickets.set(asset,(tickets.get(asset)??0)+1);setChoices(old=>{if(!old.has(asset))return old;const next=new Map(old);next.delete(asset);return next;});},
    adapt(base:TilesSource):TilesSource {
      const item=(value:GridItem|null)=>{if(!value)return null;const display=choices().get(Number(value.id));return display?{...value,imageKey:displayVariantKey(display.target),exportVariant:display.target,aspect:clampDisplayAspect(display.natural.width,display.natural.height)}:value;};
      return {...base,itemAt:index=>item(base.itemAt(index)),itemById:id=>item(base.itemById(id)),
        naturalOf:id=>choices().get(Number(id))?.natural??base.naturalOf(id),
        aspectOf:id=>{const natural=choices().get(Number(id))?.natural;return natural?clampDisplayAspect(natural.width,natural.height):base.aspectOf(id);},
        ensureNatural:entries=>base.ensureNatural(entries.filter(entry=>!choices().has(Number(entry.id))))};
    },
    dispose(){revision++;tickets.clear();},
  };
}
