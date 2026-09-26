import assert from "node:assert/strict";
import test from "node:test";
import {
  createExportStore,
  EXPORT_PRESETS_KEY,
  type ExportStoreDeps,
} from "./store.ts";
import { createExportPreferences } from "../../lib/export-prefs.ts";
import {
  serializePresets,
  variantKey,
  type AssetVariants,
  type ExportPreset,
  type VariantSnapshot,
} from "../../lib/export-model.ts";
const preset: ExportPreset = {
  id: "preset-a",
  name: "网页",
  format: "jpeg",
  quality: 90,
  maxEdge: 0,
  sizeMode: "original",
  percent: 100,
  directory: "C:\\输出",
  template: ":FILENAME",
  existingFile: "append",
};
const variants = (ids: readonly number[]): AssetVariants[] =>
  ids.map((assetId) => ({
    assetId,
    variants: ["sooc", "latest", "issue:1"].map((variant) => ({
      reference: { assetId, variant },
      name: variant,
      sourceBase: "sooc",
      profileHash: null,
      relPath: "photos/中文.jpg",
    })),
  }));
const snapshot = (reference: {
  assetId: number;
  variant: string;
}): VariantSnapshot => ({
  reference,
  name: reference.variant,
  profileHash: reference.variant,
  relPath: "photos/中文.jpg",
  sourceSignature: "sig",
  stack: { values: { exposure: 1 } },
});
function setup(patch: Partial<ExportStoreDeps> = {}) {
  const writes: string[] = [];
  const s = createExportStore({
    getSetting: async (key) => {
      assert.equal(key, EXPORT_PRESETS_KEY);
      return serializePresets([preset]);
    },
    setSetting: async (key, value) => {
      assert.equal(key, EXPORT_PRESETS_KEY);
      writes.push(value);
    },
    variants: async (_, ids) => variants(ids),
    snapshots: async (_, refs) => refs.map(snapshot),
    validate: async () => ({ errors: {}, warnings: [] }),
    id: () => "preset-b",
    preferences: createExportPreferences({
      getItem: () => null,
      setItem: () => {},
    }),
    ...patch,
  });
  s.context("repo", "photos");
  return { s, writes };
}
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
test("one asset contributes several independently selectable issues", async () => {
  const { s } = setup();
  await s.ready;
  await s.ensure([1]);
  await s.selectIssue({ assetId: 1, variant: "sooc" }, "toggle", [1]);
  await s.selectIssue({ assetId: 1, variant: "issue:1" }, "toggle", [1]);
  assert.equal(s.selection().ids.size, 2);
  await s.selectIssue({ assetId: 1, variant: "latest" }, "replace", [1]);
  assert.deepEqual(
    [...s.selection().ids],
    [variantKey("repo", { assetId: 1, variant: "latest" })],
  );
  s.dispose();
});
test("Shift reads unloaded pages and respects every issue between anchor and target", async () => {
  const { s } = setup();
  await s.ensure([1]);
  await s.selectIssue({ assetId: 1, variant: "latest" }, "replace", [1, 2, 3]);
  await s.selectIssue({ assetId: 3, variant: "sooc" }, "range", [1, 2, 3]);
  assert.equal(s.selection().ids.size, 9);
  assert.equal(s.variants().size, 3);
  s.dispose();
});
test("asset toggle selects/removes its issue group, control replace only that group", async () => {
  const { s } = setup();
  await s.selectAssets([1], "toggle", [1, 2]);
  assert.equal(s.selection().ids.size, 3);
  await s.selectAssets([2], "toggle", [1, 2]);
  assert.equal(s.selection().ids.size, 6);
  await s.selectAssets([1], "toggle", [1, 2]);
  assert.equal(s.selection().ids.size, 3);
  await s.selectAssets([1], "replace", [1, 2]);
  assert.ok(
    [...s.selection().ids].every(
      (key) => (JSON.parse(key) as unknown[])[1] === 1,
    ),
  );
  s.dispose();
});
test("overlapping requests deduplicate, chunk at 128, cache negative result", async () => {
  const calls: number[][] = [];
  const wait = deferred<AssetVariants[]>();
  const { s } = setup({
    variants: async (_, ids) => {
      calls.push([...ids]);
      return calls.length === 1 ? wait.promise : [];
    },
  });
  const a = s.ensure([1, 2]);
  const b = s.ensure([2, 3]);
  wait.resolve(variants([1, 2]));
  await Promise.all([a, b]);
  assert.deepEqual(calls, [[1, 2], [3]]);
  await s.ensure([3]);
  assert.equal(calls.length, 2);
  await s.ensure(Array.from({ length: 257 }, (_, i) => i + 10));
  assert.deepEqual(
    calls.slice(2).map((x) => x.length),
    [128, 128, 1],
  );
  s.dispose();
});
test("late response after scope switch and clear cannot resurrect selection", async () => {
  const d = deferred<AssetVariants[]>();
  const { s } = setup({ variants: async () => d.promise });
  const pending = s.selectAssets([1], "replace", [1]);
  s.clear();
  d.resolve(variants([1]));
  await pending;
  assert.equal(s.selection().ids.size, 0);
  const d2 = deferred<AssetVariants[]>();
  const { s: other } = setup({ variants: async () => d2.promise });
  const p = other.ensure([1]);
  other.context("other", "scope");
  d2.resolve(variants([1]));
  await p;
  assert.equal(other.variants().size, 0);
  s.dispose();
  other.dispose();
});
test("new gesture wins over slow range; scope cycle clears hidden selection", async () => {
  const d = deferred<AssetVariants[]>();
  const { s } = setup({
    variants: async (_, ids) => (ids.includes(2) ? d.promise : variants(ids)),
  });
  await s.selectAssets([1], "replace", [1, 2]);
  const p = s.selectAssets([2], "range", [1, 2]);
  await s.selectIssue({ assetId: 1, variant: "sooc" }, "replace", [1, 2]);
  d.resolve(variants([2]));
  await p;
  assert.equal(s.selection().ids.size, 1);
  s.cycleScope();
  assert.equal(s.selection().ids.size, 0);
  assert.equal(s.preferences.value().scope, "issues");
  s.dispose();
});
test("issue enqueue snapshots, deduplicates and private queues survive context changes without any persistence", async () => {
  const { s, writes } = setup();
  await s.ready;
  s.choosePreset(preset.id);
  await s.selectAssets([1], "replace", [1]);
  await s.enqueue("C:\\库");
  assert.equal(s.progress(preset.id).total, 3);
  await s.enqueue("C:\\库");
  assert.equal(s.progress(preset.id).total, 3);
  assert.equal(writes.length, 0);
  assert.equal(s.processing(), false);
  assert.equal(s.enabled().size, 0);
  s.context("other", "other");
  assert.equal(s.progress(preset.id).total, 3);
  s.edit({ ...preset, name: "第二个" });
  await s.save();
  await s.selectAssets([1], "replace", [1]);
  await s.enqueue("C:\\另一个库");
  assert.equal(s.progress("preset-b").total, 3);
  assert.equal(s.progress(preset.id).total, 3);
  s.reset();
  assert.equal(s.queues().size, 0);
  assert.equal(s.presets().length, 2);
  s.dispose();
});
test("newest batch at top; captured objects independent; saved parameter changes leave old entries intact", async () => {
  const captured = snapshot({ assetId: 1, variant: "latest" });
  const { s } = setup({ snapshots: async () => [captured] });
  await s.ready;
  s.choosePreset(preset.id);
  await s.ensure([1]);
  await s.selectIssue(captured.reference, "replace", [1]);
  await s.enqueue("root");
  (captured.stack as { values: { exposure: number } }).values.exposure = 9;
  assert.equal(
    (
      s.queues().get(preset.id)![0]!.snapshot.stack as {
        values: { exposure: number };
      }
    ).values.exposure,
    1,
  );
  s.edit({ quality: 20 });
  await s.save();
  assert.equal(s.queues().get(preset.id)![0]!.preset.quality, 90);
  s.dispose();
});
test("partial snapshots reject whole batch, IPC errors visible, no partial queue", async () => {
  const { s } = setup({ snapshots: async () => [] });
  await s.ready;
  s.choosePreset(preset.id);
  await s.selectAssets([1], "replace", [1]);
  await s.enqueue("root");
  assert.equal(s.queues().size, 0);
  assert.ok(s.error());
  assert.equal(s.busy(), false);
  s.dispose();
});
test("save trims/normalizes names, overwrites by existing stable ID and persists only presets", async () => {
  const { s, writes } = setup();
  await s.ready;
  s.edit({ ...preset, id: "new", name: " 网页 ", quality: 80 });
  await s.save();
  assert.equal(s.selectedPreset()?.id, preset.id);
  assert.equal(s.presets().length, 1);
  assert.equal(s.dirty(), false);
  assert.doesNotMatch(writes[0]!, /enabled|queues/);
  s.choosePreset(null);
  assert.equal(s.draft().name, "");
  s.dispose();
});
test("invalid settings and failed writes do not claim successful save", async () => {
  const { s } = setup({
    setSetting: async () => {
      throw Error("disk denied");
    },
  });
  await s.ready;
  s.edit({ ...preset, quality: 0 });
  await s.save();
  assert.ok(s.validation().errors.quality);
  s.edit({ quality: 70 });
  await s.save();
  assert.match(s.error() ?? "", /denied/);
  assert.equal(s.presets()[0]?.quality, 90);
  s.dispose();
});
test("new App state restores only presets: no queues or switches", async () => {
  const { s } = setup();
  await s.ready;
  assert.equal(s.queues().size, 0);
  assert.equal(s.enabled().size, 0);
  assert.equal(s.processing(), false);
  s.dispose();
});

test("日期/时间片使用共享整组开关，焦点移动不破坏 issue 多选", async () => {
  const { s } = setup();
  await s.group([1, 2]);
  assert.equal(s.selection().ids.size, 6);
  s.focusIssue({ assetId: 2, variant: "latest" });
  assert.equal(
    s.selection().anchor,
    variantKey("repo", { assetId: 2, variant: "latest" }),
  );
  assert.equal(s.selection().ids.size, 6);
  await s.group([1]);
  assert.equal(s.selection().ids.size, 3);
  await s.group([1]);
  assert.equal(s.selection().ids.size, 6);
  await s.group([1, 2]);
  assert.equal(s.selection().ids.size, 0);
  s.dispose();
});

test("gallery and queue select independently; clicking pending only selects; removal is explicit",async()=>{
 const {s}=setup();await s.ready;s.choosePreset(preset.id);await s.ensure([1]);
 await s.selectIssue({assetId:1,variant:"latest"},"toggle",[1]);
 assert(s.canEnqueue());await s.enqueue("C:/库");assert(!s.canEnqueue());assert(s.canRemove());
 const item=s.queueItems()[0]!;
 s.selectQueue(item.id,"toggle");assert.equal(s.queueSelection().ids.size,1);assert.equal(s.selection().ids.size,1);
 s.selectQueue(item.id,"toggle");assert.equal(s.queueSelection().ids.size,0);assert.equal(s.queueItems().length,1);
 assert(!s.canRemove());s.selectQueue(item.id,"replace");s.removeSelected();
 assert.equal(s.queueItems().length,0);assert.equal(s.selection().ids.size,1);
 s.focusArea("gallery");assert(s.canEnqueue());s.clear();assert(!s.canEnqueue());s.dispose();
});
test("running and done lock both views, preset switches isolate status and reset ignores late callback",async()=>{
 const {s}=setup({getSetting:async()=>serializePresets([preset,{...preset,id:"other",name:"另一队列"}])});
 await s.ready;s.choosePreset(preset.id);await s.ensure([1]);
 const ref={assetId:1,variant:"latest"};await s.selectIssue(ref,"replace",[1]);await s.enqueue("C:/库");
 const item=s.queueItems()[0]!;s.selectQueue(item.id,"replace");s.setQueueStatus(item.id,"running");
 assert.equal(s.selection().ids.size,0);assert.equal(s.queueSelection().ids.size,0);
 assert(s.locked(s.listFor(1).find(v=>v.reference.variant==="latest")!));
 await s.selectIssue(ref,"toggle",[1]);s.selectQueue(item.id,"toggle");assert.equal(s.selection().ids.size,0);assert.equal(s.queueSelection().ids.size,0);
 s.setQueueStatus(item.id,"done");assert(!s.canRemove());
 s.choosePreset("other");assert(!s.locked(s.listFor(1).find(v=>v.reference.variant==="latest")!));
 await s.selectIssue(ref,"toggle",[1]);assert(s.canEnqueue());s.choosePreset(preset.id);assert(!s.canEnqueue());
 s.reset();s.setQueueStatus(item.id,"done");assert.equal(s.queues().size,0);assert.equal(s.selection().ids.size,0);assert.equal(s.enabled().size,0);s.dispose();
});

test("catalog invalidation reloads metadata even if virtual range stays unchanged",async()=>{
 let named=true;const {s}=setup({variants:async(_,ids)=>variants(ids).map(p=>({...p,variants:named?p.variants:p.variants.filter(v=>v.reference.variant==="sooc")}))});
 await s.ready;await s.ensure([1]);assert.equal(s.listFor(1).length,3);
 named=false;s.invalidate([1]);await s.ensure([1]);assert.deepEqual(s.listFor(1).map(v=>v.reference.variant),["sooc"]);s.dispose();
});

test("reset isolates an in-flight capture and a delayed range selection",async()=>{
 const capture=deferred<VariantSnapshot[]>();const {s}=setup({snapshots:()=>capture.promise});
 await s.ready;s.choosePreset(preset.id);await s.ensure([1]);await s.selectIssue({assetId:1,variant:"latest"},"replace",[1]);
 const enqueue=s.enqueue("C:/库");s.reset();capture.resolve([snapshot({assetId:1,variant:"latest"})]);await enqueue;
 assert.equal(s.queues().size,0);assert.equal(s.selection().ids.size,0);s.dispose();
 const summaries=deferred<AssetVariants[]>(),state=setup({variants:()=>summaries.promise}).s;await state.ready;
 const selecting=state.group([1],false);state.reset();summaries.resolve(variants([1]));await selecting;
 assert.equal(state.selection().ids.size,0);state.dispose();
});

test("select-all and Shift only cover photos admitted by the active filter",async()=>{
 const {s}=setup({variants:async(_,ids)=>variants(ids).map(p=>p.assetId===2?{...p,variants:p.variants.filter(v=>v.reference.variant==="sooc")}:p)});
 await s.ready;await s.ensure([1,2]);s.preferences.update({scope:"issues"});await s.group([1,2],false);
 assert.equal(s.selection().ids.size,3);assert([...s.selection().ids].every(key=>JSON.parse(key)[1]===1));s.dispose();
});

test("runtime is authoritative, rejects older events and clears locked selections", async()=>{
  const calls:string[]=[];
  const {s,writes}=setup({runtime:async(action)=>{calls.push(action);return {revision:0,generation:0,queues:{},enabled:[]};},subscribe:async()=>()=>{}});
  await s.ready;s.choosePreset(preset.id);await s.ensure([1]);
  await s.selectIssue({assetId:1,variant:"issue:1"},"toggle",[1]);
  const entry={id:"job",repositoryId:"repo",root:"root",snapshot:snapshot({assetId:1,variant:"issue:1"}),preset,status:"running" as const,error:null,sequence:1};
  s.applyRuntime({revision:2,generation:0,queues:{[preset.id]:[entry]},enabled:[preset.id]});
  assert(s.processing());assert.equal(s.selection().ids.size,0);
  s.applyRuntime({revision:1,generation:0,queues:{},enabled:[]});assert.equal(s.queueItems().length,1);
  s.applyRuntime({revision:3,generation:1,queues:{},enabled:[]});assert(!s.processing());
  s.applyRuntime({revision:2,generation:0,queues:{[preset.id]:[entry]},enabled:[preset.id]});assert.equal(s.queueItems().length,0);
  assert.equal(writes.length,0,"events never persist queues");s.dispose();
});
test("empty-enabled state has no activity and limit is enforced before runtime call",async()=>{
 const calls:string[]=[];const {s}=setup({runtime:async(action)=>{calls.push(action);return {revision:0,generation:0,queues:{},enabled:[]};}});await s.ready;s.choosePreset(preset.id);
 s.applyRuntime({revision:1,generation:0,queues:{},enabled:["b","c","d","e"]});assert(!s.processing());s.toggleRun();assert(s.limitOpen());assert(!calls.includes("enable"));
});

test("reset immediately hides queues and rejects old-generation events before acknowledgement",async()=>{
 const reset=deferred<import("../../api/export.ts").ExportQueueView>();
 const {s}=setup({runtime:async(action)=>action==="reset"?reset.promise:{revision:0,generation:0,queues:{},enabled:[]}});await s.ready;s.choosePreset(preset.id);
 const entry={id:"job",repositoryId:"repo",root:"root",snapshot:snapshot({assetId:1,variant:"issue:1"}),preset,status:"running" as const,error:null,sequence:1};
 s.applyRuntime({revision:1,generation:0,queues:{[preset.id]:[entry]},enabled:[preset.id]});s.reset();assert.equal(s.queueItems().length,0);assert(!s.canRun());
 s.applyRuntime({revision:2,generation:0,queues:{[preset.id]:[entry]},enabled:[preset.id]});assert.equal(s.queueItems().length,0);
 reset.resolve({revision:3,generation:1,queues:{},enabled:[]});await Promise.resolve();await Promise.resolve();assert(!s.processing());s.dispose();
});
test("failed event subscription still restores saved presets without unhandled rejection",async()=>{
 const {s}=setup({subscribe:async()=>{throw new Error("subscription failed")}});await s.ready;assert.equal(s.presets().length,1);assert(!s.loading());assert.match(s.error()??"",/subscription failed/);s.dispose();
});

test("metadata reread is atomic and retains unchanged objects; overlapping invalidations cannot restore stale lists", async()=>{
 const first=variants([1,2]); let phase=0;
 const slow=deferred<AssetVariants[]>();
 const {s}=setup({variants:async(_,ids)=>phase===0?first.filter(p=>ids.includes(p.assetId)):phase===1?slow.promise:variants(ids).map(p=>({...p,variants:p.variants.slice(0,1)}))});
 await s.ensure([1,2]);const original=s.variants(),one=s.variants().get(1),two=s.variants().get(2);
 phase=1;const pending=s.invalidate([1]);
 assert.equal(s.variants(),original);assert.equal(s.variants().get(1),one,"never collapse while loading");
 phase=2;await s.invalidate([1]);assert.equal(s.listFor(1).length,1);assert.equal(s.variants().get(2),two);
 slow.resolve(first);await pending;assert.equal(s.listFor(1).length,1,"late old batch cannot restore deleted issues");
 const stable=s.variants();await s.invalidate([1]);assert.equal(s.variants(),stable,"same data keeps map/variant identities");s.dispose();
});
test("failed metadata reread retains the prior complete list and can retry", async()=>{
 let fail=false;const {s}=setup({variants:async(_,ids)=>{if(fail)throw Error("offline");return variants(ids);}});
 await s.ensure([1]);const old=s.variants();fail=true;await s.invalidate([1]);assert.equal(s.variants(),old);assert.match(s.error()!,/offline/);
 fail=false;await s.ensure([1]);assert.equal(s.listFor(1).length,3);s.dispose();
});
test("v1 presets migrate once to v3, preserve identity and replace TIFF with lossless PNG",async()=>{
 const storage=new Map<string,string>([["export.presets.v1",JSON.stringify({version:1,presets:[{...preset,format:"tiff",maxEdge:3000}]})]]);
 const writes:string[]=[];
 const deps={getSetting:async(key:string)=>storage.get(key)??null,setSetting:async(key:string,value:string)=>{writes.push(key);storage.set(key,value);}};
 const {s}=setup(deps);await s.ready;
 assert.equal(s.presets()[0]?.id,preset.id);assert.equal(s.presets()[0]?.format,"png");assert.equal(s.presets()[0]?.sizeMode,"maxEdge");
 assert.equal(s.presets()[0]?.percent,100);assert.deepEqual(writes,[EXPORT_PRESETS_KEY]);s.dispose();
 const {s:next}=setup(deps);await next.ready;assert.deepEqual(writes,[EXPORT_PRESETS_KEY]);next.dispose();
});
test("saving a renamed selected preset creates another while an existing name updates only that preset",async()=>{
 const {s}=setup();await s.ready;s.choosePreset(preset.id);s.edit({name:"新名称"});await s.save();assert.equal(s.presets().length,2);
 assert.equal(s.presets().find(p=>p.id===preset.id)?.name,preset.name);
 s.edit({name:preset.name,quality:45});await s.save();assert.equal(s.presets().length,2);assert.equal(s.selectedPreset()?.id,preset.id);assert.equal(s.selectedPreset()?.quality,45);s.dispose();
});

test("form name matching uses the same trim/NFC rule as save, independent of selected preset",async(t)=>{
 t.mock.timers.enable({apis:["setTimeout"]});
 const {s}=setup();await s.ready;s.choosePreset(preset.id);
 assert.equal(s.matchingPreset()?.id,preset.id);
 s.edit({name:"另一个"});t.mock.timers.tick(1500);assert.equal(s.matchingPreset(),null);assert.equal(s.selectedPreset()?.id,preset.id);
 s.edit({name:"  网页  "});t.mock.timers.tick(1500);assert.equal(s.matchingPreset()?.id,preset.id);await s.save();assert.equal(s.presets().length,1);
 s.edit({name:"Café"});await s.save();s.edit({name:" Cafe\u0301 "});assert.equal(s.matchingPreset()?.name,"Café");
 s.edit({name:"   "});t.mock.timers.tick(1500);assert.equal(s.matchingPreset(),null);s.dispose();
});

test("name detection debounces 1500ms, resets on typing and cancels when selecting or disposing",async(t)=>{
 t.mock.timers.enable({apis:["setTimeout"]});
 const {s}=setup();await s.ready;s.choosePreset(preset.id);assert(s.canSave());
 s.edit({name:"新的"});assert(s.nameChecking());assert(!s.canSave());assert.equal(s.matchingPreset()?.id,preset.id);
 t.mock.timers.tick(1499);assert.equal(s.matchingPreset()?.id,preset.id);
 s.edit({name:"新名称"});t.mock.timers.tick(1499);assert(s.nameChecking());
 t.mock.timers.tick(1);assert(!s.nameChecking());assert.equal(s.matchingPreset(),null);assert(s.canSave());
 s.edit({name:preset.name});s.choosePreset(preset.id);assert(!s.nameChecking());assert.equal(s.matchingPreset()?.id,preset.id);
 t.mock.timers.tick(2000);assert.equal(s.matchingPreset()?.id,preset.id);
 s.edit({name:"迟到名称"});s.dispose();t.mock.timers.tick(2000);assert.equal(s.matchingPreset()?.id,preset.id);
});

test('unsaved changes block both button and command starts; stop remains available while dirty',async()=>{
 const calls:string[]=[];const {s}=setup({runtime:async action=>{calls.push(action);return{revision:0,generation:0,queues:{},enabled:[]};}});
 await s.ready;s.choosePreset(preset.id);assert(s.canRun());s.edit({quality:42});assert(!s.canRun());s.toggleRun();assert(!calls.includes('enable'));
 await s.save();assert(s.canRun());s.toggleRun();assert.equal(calls.filter(action=>action==='enable').length,1);
 s.applyRuntime({revision:1,generation:0,queues:{},enabled:[preset.id]});s.edit({quality:24});assert(s.canRun());s.toggleRun();assert(calls.includes('disable'));s.dispose();
});


test("v2 migration preserves settings and makes collision policy explicitly append once",async()=>{
 const {existingFile:_,...legacy}=preset;
 const storage=new Map([["export.presets.v2",JSON.stringify({version:2,presets:[legacy]})]]);
 const writes:string[]=[];
 const deps={getSetting:async(key:string)=>storage.get(key)??null,setSetting:async(key:string,value:string)=>{writes.push(key);storage.set(key,value);}};
 const {s}=setup(deps);await s.ready;assert.equal(s.presets()[0]?.existingFile,"append");s.dispose();
 assert.equal(JSON.parse(storage.get(EXPORT_PRESETS_KEY)!).version,3);
 const {s:next}=setup(deps);await next.ready;assert.deepEqual(writes,[EXPORT_PRESETS_KEY]);next.dispose();
});

test("collision edits need save, queued policy stays captured and skipped is terminal",async()=>{
 const {s}=setup();await s.ready;s.choosePreset(preset.id);await s.ensure([1]);
 s.edit({existingFile:"skip"});assert(!s.canRun());await s.save();assert(s.canRun());
 await s.selectIssue({assetId:1,variant:"sooc"},"toggle",[1]);await s.enqueue("C:/Library");
 const item=s.queueItems()[0]!;assert.equal(item.preset.existingFile,"skip");
 s.edit({existingFile:"overwrite"});assert.equal(s.queueItems()[0]!.preset.existingFile,"skip");
 s.setQueueStatus(item.id,"skipped");assert(s.locked(s.listFor(1)[0]!));
 assert.deepEqual(s.progress(preset.id),{remaining:0,total:1,processing:false});
 s.focusArea("queue");s.selectAllActive([1]);assert.equal(s.queueSelection().ids.size,0);assert(!s.canRemove());s.dispose();
});

test('drag targets saved preset without switching and retains batch even as current queue runs or finishes',async()=>{
 const other={...preset,id:'second',name:'另一份输出'};
 const {s}=setup({getSetting:async()=>serializePresets([preset,other]),variants:async(_,ids)=>variants(ids).map(p=>({...p,variants:p.variants.map(v=>({...v,profileHash:v.reference.variant}))}))});
 await s.ready;s.choosePreset(preset.id);await s.group([1,2],false);
 const refs=s.selectedVariants().map(v=>v.reference),selected=[...s.selection().ids];
 assert.equal(await s.enqueue('C:/库',{presetId:preset.id,references:refs,preserveSelection:true}),6);
 for(const item of s.queueItems())s.setQueueStatus(item.id,'running');
 assert.deepEqual([...s.selection().ids],selected);assert(s.selectedVariants().every(v=>s.locked(v)));
 assert.equal(await s.enqueue('C:/库',{presetId:other.id,references:refs,preserveSelection:true}),6);
 assert.equal(s.selectedPreset()?.id,preset.id);assert.equal(s.progress(other.id).total,6);
 for(const item of s.queueItems())s.setQueueStatus(item.id,'done');
 assert.deepEqual([...s.selection().ids],selected);
 assert.equal(await s.enqueue('C:/库',{presetId:other.id,references:refs,preserveSelection:true}),0);
 assert.equal(await s.enqueue('C:/库',{presetId:'missing',references:refs,preserveSelection:true}),0);
 s.clear();assert.equal(s.selection().ids.size,0);s.dispose();
});

test('pending drag snapshots cannot enqueue after directory switch or reset',async()=>{
 for(const cancel of ['context','reset']) {
  const pending=deferred<VariantSnapshot[]>();const {s}=setup({snapshots:async()=>pending.promise});
  await s.ready;await s.group([1],false);const refs=s.selectedVariants().map(v=>v.reference);
  const queued=s.enqueue('root',{presetId:preset.id,references:refs,preserveSelection:true});
  if(cancel==='context')s.context('other','dir');else s.reset();
  pending.resolve(refs.map(snapshot));assert.equal(await queued,0);assert.equal(s.queues().size,0);s.dispose();
 }
});

test('Ctrl+A follows active area; bottom Delete excludes running, done and skipped and preserves upper selection',async()=>{
 const {s}=setup();await s.ready;s.choosePreset(preset.id);await s.ensure([1,2]);
 s.focusArea('gallery');s.selectAllActive([1,2]);await Promise.resolve();await Promise.resolve();
 assert.equal(s.selection().ids.size,6);await s.enqueue('root');
 const items=s.queueItems();s.setQueueStatus(items[0]!.id,'running');s.setQueueStatus(items[1]!.id,'done');s.setQueueStatus(items[2]!.id,'skipped');s.setQueueStatus(items[3]!.id,'failed');
 const upper=[...s.selection().ids];s.focusArea('queue');s.selectAllActive([1,2]);
 assert.deepEqual([...s.queueSelection().ids],items.slice(3).map(i=>i.id));assert(s.canRemove());s.removeSelected();
 assert.equal(s.queueItems().length,3);assert.deepEqual([...s.selection().ids],upper);assert.equal(s.queueSelection().ids.size,0);
 s.selectAllActive([]);assert.equal(s.queueSelection().ids.size,0);s.dispose();
});
