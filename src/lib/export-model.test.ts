import assert from "node:assert/strict";
import test from "node:test";
import {
  presetErrors,
  exportGalleryState,
  queueProgress,
  readPresets,
  serializePresets,
  variantKey,
  variantAssetId,
  visibleVariants, admitsPhoto, mainVariant, orderedIssues,
  type ExportPreset,
  type ExportQueueItem,
  type VariantSummary,
} from "./export-model.ts";
const preset: ExportPreset = {
  id: "stable",
  name: "中文 📷",
  format: "jpeg",
  quality: 90,
  maxEdge: 0,
  sizeMode: "original",
  percent: 100,
  directory: "C:\\输出",
  template: ":FILENAME",
  existingFile: "append",
};
test("variant identity distinguishes repositories, assets and multiple issues", () => {
  assert.notEqual(
    variantKey("one", { assetId: 1, variant: "issue:1" }),
    variantKey("one", { assetId: 1, variant: "issue:2" }),
  );
  assert.notEqual(
    variantKey("one", { assetId: 1, variant: "latest" }),
    variantKey("two", { assetId: 1, variant: "latest" }),
  );
});
test("variantAssetId 与 variantKey 成对：坏输入一律 null 不抛，位置契约只由这一对拥有", () => {
  // 正常往返（中文仓名、variant 里的冒号、assetId = 0）
  assert.equal(
    variantAssetId(variantKey("库/中文", { assetId: 42, variant: "issue:7" })),
    42,
  );
  assert.equal(
    variantAssetId(variantKey("", { assetId: 0, variant: "sooc" })),
    0,
    "0 是合法 assetId（别把 falsy 当失败）",
  );

  // 坏输入：旧写法（`JSON.parse(key)[1]`）这些会直接抛，而调用点分别在 createMemo 与
  // setSelection 里 —— 抛出去就是打坏渲染 / 卡住状态更新。现在一律 null。
  assert.equal(variantAssetId(""), null);
  assert.equal(variantAssetId("null"), null);
  assert.equal(variantAssetId("不是 JSON"), null);
  assert.equal(variantAssetId("[]"), null);
  assert.equal(variantAssetId('["repo"]'), null);
  assert.equal(variantAssetId('"repo"'), null);
  assert.equal(variantAssetId('{"0":"repo"}'), null, "对象不是元组");
  assert.equal(
    variantAssetId('["repo","42","latest"]'),
    null,
    "字符串数字不算 assetId",
  );

  // 边界数值：非整数 / 溢出成 Infinity / 超安全整数范围
  assert.equal(variantAssetId('["repo",2.5,"latest"]'), null);
  assert.equal(variantAssetId('["repo",1e999,"latest"]'), null);
  assert.equal(
    variantAssetId('["repo",9007199254740993,"latest"]'),
    null,
    "超安全整数范围（解析时已失真，不能当作真 assetId 用）",
  );
  assert.equal(
    variantAssetId('["repo",-1,"latest"]'),
    -1,
    "负整数仍被取回 —— 上游不会造，但这里不做业务校验",
  );

  // 位置契约：assetId 在第 1 位；多出的尾巴不影响
  assert.equal(variantAssetId('["repo",42,"latest","extra"]'), 42);
});
test("photo filters never restrict the admitted photo's issue list", () => {
  const items=[{reference:{assetId:1,variant:"sooc"},profileHash:"base",sourceBase:"sooc"},
    {reference:{assetId:1,variant:"latest"},main:true,edited:true,profileHash:"edited",sourceBase:"sooc"},
    {reference:{assetId:1,variant:"issue:1"},profileHash:"edited",sourceBase:"sooc"}] as VariantSummary[];
  for(const scope of ["all","issues","edited"] as const)assert.equal(visibleVariants(items,scope).length,3);
  assert(admitsPhoto(items,"edited"));assert(admitsPhoto(items,"issues"));
  assert.equal(mainVariant(items)?.reference.variant,"latest");
  assert.deepEqual(orderedIssues(items,()=>false).map(v=>v.reference.variant),["sooc"]);
  const reset=items.map(v=>v.main?{...v,edited:false}:v);
  assert(!admitsPhoto(reset,"edited"));assert(admitsPhoto(reset,"issues"));
  assert(admitsPhoto([],"all"));assert(!admitsPhoto([],"issues"));
});
test("promoted issues precede newest-first remaining issues and unselected SOOC is last",()=>{
  const items=[{reference:{assetId:1,variant:"latest"},main:true},...Array.from({length:8},(_,i)=>({reference:{assetId:1,variant:`issue:${i}`},createdAt:i})),{reference:{assetId:1,variant:"sooc"}}] as VariantSummary[];
  assert.equal(orderedIssues(items,v=>v.reference.variant==="issue:0")[0]?.reference.variant,"issue:0");
  assert.equal(orderedIssues(items,()=>false)[orderedIssues(items,()=>false).length-1]?.reference.variant,"sooc");
});
test("queue counter includes completed in total; failure remains unfinished; only actual running animates", () => {
  const entries = ["pending", "done", "failed"].map(
    (status) => ({ status }) as ExportQueueItem,
  );
  assert.deepEqual(queueProgress(entries), {
    remaining: 2,
    total: 3,
    processing: false,
  });
  assert.deepEqual(queueProgress([]), {
    remaining: 0,
    total: 0,
    processing: false,
  });
  assert.equal(
    queueProgress([{ status: "running" } as ExportQueueItem]).processing,
    true,
  );
  assert.deepEqual(queueProgress([{ status: "done" } as ExportQueueItem]), {
    remaining: 0,
    total: 1,
    processing: false,
  });
});
test("presets persist explicit settings only, never queue, processing or enabled", () => {
  const payload = serializePresets([
    { ...preset, queue: [1], enabled: true, status: "running" } as ExportPreset,
  ]);
  assert.deepEqual(readPresets(payload), [preset]);
  assert.doesNotMatch(payload, /queue|enabled|running/);
});
test("damaged, future version, primitive, duplicates and invalid fields rejected", () => {
  for (const raw of [
    null,
    "bad",
    "[]",
    "null",
    "2",
    '{"version":2,"presets":[]}',
  ])
    assert.deepEqual(readPresets(raw), []);
  assert.deepEqual(
    readPresets(
      JSON.stringify({
        version: 1,
        presets: [
          preset,
          { ...preset, id: "two" },
          { ...preset, name: "other" },
          { ...preset, id: "bad", name: "third", quality: 0 },
        ],
      }),
    ),
    [preset],
  );
});
test("preset Unicode, number edges, NUL and fractional/NaN values", () => {
  assert.deepEqual(presetErrors(preset), {});
  assert.equal(
    presetErrors({ ...preset, quality: 1, maxEdge: 65535 }).quality,
    undefined,
  );
  assert.ok(
    presetErrors({
      ...preset,
      name: "\0",
      quality: NaN,
      maxEdge: 2.4,
      directory: "\0",
      template: " ",
    }).name,
  );
  assert.ok(presetErrors({ ...preset, name: "字".repeat(129) }).name);
  for (const q of [0, 101, 1.5, Infinity])
    assert.ok(presetErrors({ ...preset, quality: q }).quality);
});


test("gallery with photos allows virtual paging in every scope, refresh never masks existing content", () => {
  const state = {repository:"repo",scope:"photos",count:1,loading:false,error:null};
  assert.equal(exportGalleryState(state),null);
  assert.equal(exportGalleryState({...state,loading:true}),null);
  assert.equal(exportGalleryState({...state,count:0}),"empty");
  assert.equal(exportGalleryState({...state,count:0,loading:true}),"loading");
  assert.equal(exportGalleryState({...state,count:0,error:"denied"}),"error");
  assert.equal(exportGalleryState({...state,repository:null}),"repository");
  assert.equal(exportGalleryState({...state,scope:null}),"directory");
});

test("quality control only applies to lossy codecs",async()=>{
 const {formatSupportsQuality}=await import("./export-model.ts");
 for(const format of ["jpeg","webp","avif"] as const)assert(formatSupportsQuality(format));
 for(const format of ["png"] as const)assert(!formatSupportsQuality(format));
});

test("lossless quality is inactive during validation while lossy quality must stay bounded",()=>{
 for(const format of ["png"] as const)assert.equal(presetErrors({...preset,format,quality:0}).quality,undefined);
 for(const format of ["jpeg","webp","avif"] as const)assert.equal(presetErrors({...preset,format,quality:0}).quality,"quality");
});

test("four public formats and versioned size modes reject invalid bounds and preserve percent",()=>{
 for(const [mode,value] of [["percent",0],["percent",101],["percent",1.5],["percent",NaN],["maxEdge",0],["maxEdge",65536]] as const){
  const p={...preset,sizeMode:mode,...(mode==="percent"?{percent:value}:{maxEdge:value})};
  assert(Object.keys(presetErrors(p)).length>0);
 }
 const percent={...preset,sizeMode:"percent" as const,percent:50};
 assert.deepEqual(readPresets(serializePresets([percent])),[percent]);
 assert.equal(presetErrors({...preset,format:"tiff" as never}).format,"format");
 assert.deepEqual(readPresets(JSON.stringify({version:2,presets:[{...preset,format:"tiff"}]})),[]);
 const {sizeMode:_,percent:__,...legacy}=preset;
 assert.deepEqual(readPresets(JSON.stringify({version:1,presets:[{...legacy,format:"tiff",maxEdge:2048}]})),[{...preset,format:"png",sizeMode:"maxEdge",maxEdge:2048}]);
});


test("file collision choices migrate append, preserve explicit v3 policies and reject unknown values",()=>{
 const {existingFile:_,...legacy}=preset;
 for(const version of [1,2])assert.equal(readPresets(JSON.stringify({version,presets:[legacy]}))[0]?.existingFile,"append");
 for(const existingFile of ["overwrite","skip","append"] as const){
  const p={...preset,existingFile};assert.deepEqual(readPresets(serializePresets([p])),[p]);
 }
 for(const existingFile of [undefined,"bad",null])assert.deepEqual(readPresets(JSON.stringify({version:3,presets:[{...preset,existingFile}]})),[]);
 assert.deepEqual(queueProgress([{status:"skipped"} as ExportQueueItem]),{remaining:0,total:1,processing:false});
});
