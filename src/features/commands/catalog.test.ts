import assert from "node:assert/strict";
import { detectConflicts } from "../../lib/commands.ts";
import test from "node:test";

test("色彩面板、输入恢复、批量指定、打样和警告共用命令入口且不占默认键",()=> {
  const calls:string[]=[];
  const deps={editor:{active:()=>true,hasPhoto:()=>true,color:{open:()=>calls.push("open"),restore:()=>calls.push("restore"),batch:()=>calls.push("batch"),proof:()=>calls.push("proof"),warning:()=>calls.push("warning")}}} as unknown as CommandDeps;
  const registry=createCommandRegistry(deps);
  for(const [id,name] of [["open","open"],["restore","restore"],["batch","batch"],["proof","proof"],["gamutWarning","warning"]]) {
    const command=registry.find(c=>c.id===`editor.color.${id}`)!;
    assert.equal(command.defaultKey,undefined);assert.equal(command.when?.(),true);command.run();assert.equal(calls[calls.length - 1],name);
  }
});

import {
  createCommandRegistry,
  type CommandDeps,
  type CommandFlow,
} from "./catalog.ts";

test("compare Enter：按当前 flow 切各自的胶片带范围", () => {
  let flow: CommandFlow = "browse";
  const calls: string[] = [];
  const deps = {
    flow: () => flow,
    viewer: { comparing: () => true },
    browse: { toggleCompareStrip: () => calls.push("browse") },
    import: { toggleCompareStrip: () => calls.push("import") },
  } as unknown as CommandDeps;

  const command = createCommandRegistry(deps).find(
    (candidate) => candidate.id === "viewer.compareOnly",
  );
  assert.ok(command, "compare 胶片带切换必须登记进命令注册表");
  assert.equal(command.defaultKey, "Enter");
  assert.equal(command.when?.(), true);

  command.run();
  flow = "import";
  command.run();
  assert.deepEqual(calls, ["browse", "import"]);
});

test("compare Enter：非对比态不适用", () => {
  const deps = {
    flow: () => "browse" as const,
    viewer: { comparing: () => false },
  } as unknown as CommandDeps;
  const command = createCommandRegistry(deps).find(
    (candidate) => candidate.id === "viewer.compareOnly",
  );
  assert.equal(command?.when?.(), false);
});

test("SOOC/RAW 编辑源命令共用工具栏的选择动作且没有误触热键", () => {
  const calls: string[] = [];
  const deps = {
    editor: {
      active: () => true,
      hasPhoto: () => true,
      setBase: (base: string) => calls.push(base),
    },
  } as unknown as CommandDeps;
  const commands = createCommandRegistry(deps);
  const sooc = commands.find((candidate) => candidate.id === "editor.base.sooc");
  const raw = commands.find((candidate) => candidate.id === "editor.base.raw");
  assert.ok(sooc);
  assert.ok(raw);
  assert.equal(sooc.defaultKey, undefined);
  assert.equal(raw.defaultKey, undefined);
  assert.equal(sooc.when?.(), true);
  sooc.run();
  raw.run();
  assert.deepEqual(calls, ["sooc", "raw"]);
});

test("W5 三工具默认键位在命令注册表内且不冲突", () => {
  const deps = {
    editor: { active: () => true, hasPhoto: () => true, toggleTool: () => {} },
  } as unknown as CommandDeps;
  const commands = createCommandRegistry(deps);
  const expected = { "editor.tool.crop": "C", "editor.tool.rotate": "R", "editor.tool.compare": "B" };
  for (const [id, chord] of Object.entries(expected)) {
    assert.equal(commands.find((command) => command.id === id)?.defaultKey, chord);
  }
  const conflicts = detectConflicts(commands, {});
  assert.deepEqual(conflicts.filter((issue) => issue.blocking &&
    issue.commandIds.some((id) => id in expected)), []);
});

test('自动调整命令复用编辑动作且默认不占热键（不进菜单：工具栏是入口）', () => {
  let applied = 0;
  const deps = {editor:{active:()=>true, hasPhoto:()=>true, autoAdjust:()=>{applied++;}}} as unknown as CommandDeps;
  const command = createCommandRegistry(deps).find(item=>item.id==='editor.develop.autoAdjust');
  assert.ok(command);
  assert.equal(command.defaultKey, undefined);
  // 2026-10-09「弱菜单」整理：编辑器工具栏已有「自动调整」按钮，菜单不再重复入口
  // （命令仍在注册表里：Ctrl+K 可搜、快捷键设置里可绑键）
  assert.equal(command.menu, undefined);
  command.run();
  assert.equal(applied, 1);
});

test("重置命令共享可用性：无调整禁用，有手动或自动调整可用，无默认键", () => {
  let available = false;
  let calls = 0;
  const deps = { editor: { active: () => true, hasPhoto: () => true, canReset: () => available, resetDevelop: () => calls++ } } as unknown as CommandDeps;
  const command = createCommandRegistry(deps).find((item) => item.id === "editor.develop.reset")!;
  assert.equal(command.defaultKey, undefined);
  assert.equal(command.when?.(), false);
  available = true;
  assert.equal(command.when?.(), true);
  command.run();
  assert.equal(calls, 1);
});

test("导出命令默认 Enter 无冲突，Esc/Ctrl+A 复用取消和全选，切工作流失效",()=>{
 let flow:CommandFlow="export";const calls:string[]=[];const deps={flow:()=>flow,viewer:{viewing:()=>false},export:{hasSelection:()=>true,canEnqueue:()=>true,enqueue:()=>calls.push("enqueue"),clearSelection:()=>calls.push("clear"),selectAll:()=>calls.push("all"),reset:()=>calls.push("reset"),canReset:()=>true,stopAll:()=>calls.push("stop"),canStop:()=>false,cycleScope:()=>calls.push("scope"),canSave:()=>true,save:()=>calls.push("save")}} as unknown as CommandDeps;
 const commands=createCommandRegistry(deps);for(const [id,key] of [["export.enqueue","Enter"],["edit.clearSelection","Esc"],["edit.selectAll","Mod+A"]]){const command=commands.find(c=>c.id===id)!;assert.equal(command.defaultKey,key);assert.equal(command.when?.(),true);command.run();}
 assert.deepEqual(calls,["enqueue","clear","all"]);assert.equal(commands.find(c=>c.id==="export.stopAll")?.enabled?.(),false);
 const conflicts=detectConflicts(commands,{});assert.deepEqual(conflicts.filter(issue=>issue.blocking&&issue.commandIds.some(id=>id.startsWith("export."))),[]);
 for(const id of ["export.reset","export.stopAll","export.scope","export.save"])assert.equal(commands.find(c=>c.id===id)?.defaultKey,undefined);
 flow="browse";assert.equal(commands.find(c=>c.id==="export.enqueue")?.when?.(),false);
});

test("export reuses Delete without a shortcut conflict and bounded zoom follows active tiles",()=>{
 let area="gallery",position=0,canRemove=false;const calls:string[]=[];
 const deps={flow:()=>"export",viewer:{viewing:()=>false},export:{canRemove:()=>canRemove,remove:()=>calls.push("remove")},display:{tileStep:()=>position,sizeBounds:()=>area==="gallery"?{min:240,max:320}:undefined,setTileStep:(p:number)=>{position=p;},commitTileStep:()=>{}}} as unknown as CommandDeps;
 const registry=createCommandRegistry(deps),del=registry.find(c=>c.id==="edit.delete")!,zoomIn=registry.find(c=>c.id==="view.tiles.zoomIn")!,zoomOut=registry.find(c=>c.id==="view.tiles.zoomOut")!;
 assert.equal(del.titleKey,"cmd.export.remove");assert.equal(del.defaultKey,"Delete");assert(!del.enabled?.());canRemove=true;del.run();assert.deepEqual(calls,["remove"]);
 assert(!zoomOut.enabled?.());position=5;assert(!zoomIn.enabled?.());area="queue";assert(zoomIn.enabled?.());zoomIn.run();assert.equal(position,6);
 assert.deepEqual(detectConflicts(registry,{}).filter(c=>c.blocking),[]);
});

test("export execution has Mod+Enter; deferred/removed features have no command; Esc is sole clear key",()=>{
 const deps={flow:()=>"export",export:{canRun:()=>true,toggleRun:()=>{}}}as unknown as CommandDeps;
 const cmds=createCommandRegistry(deps);
 assert.equal(cmds.find(c=>c.id==="export.run")?.defaultKey,"Mod+Enter");
 assert.equal(cmds.find(c=>c.id==="edit.clearSelection")?.defaultKey,"Esc");
 for(const id of ["export.preview","export.importPresets","export.exportPresets","export.failures"])assert(!cmds.some(c=>c.id===id));
 assert(!cmds.some(c=>c.defaultKey==="Mod+C"));
});

test("M5 帮助动作共用回调，F1 无冲突，低频设置明确不占热键",()=>{
 const calls:string[]=[];
 const deps={openHelp:()=>calls.push("help"),openLicenses:()=>calls.push("licenses"),openWelcome:()=>calls.push("welcome"),openUpdates:()=>calls.push("updates")} as unknown as CommandDeps;
 const commands=createCommandRegistry(deps);
 for(const [id,label] of [["help.docs","help"],["help.licenses","licenses"],["help.welcome","welcome"],["help.updates","updates"]]){
  const command=commands.find(c=>c.id===id)!;assert.equal(command.defaultKey,id==="help.docs"?"F1":undefined);assert.equal(command.enabled?.(),true);command.run();assert.equal(calls[calls.length-1],label);
 }
 assert.deepEqual(calls,["help","licenses","welcome","updates"]);
 assert.deepEqual(detectConflicts(commands,{}).filter(c=>c.blocking&&c.commandIds.includes("help.docs")),[]);
});

test("库重新查找走统一动作，文件菜单可达且明确不占热键", () => {
  let allowed = false, calls = 0;
  const registry = createCommandRegistry({ repository: { canReconnect: () => allowed, reconnect: () => { calls++; } } } as unknown as CommandDeps);
  const command = registry.find(item => item.id === "repository.reconnect");
  assert.ok(command); assert.equal(command.defaultKey, undefined);
  assert.equal(command.scope, "global"); assert.equal(command.menu, "file");
  assert.equal(command.enabled?.(), false); allowed = true; assert.equal(command.enabled?.(), true);
  command.run(); assert.equal(calls, 1);
  assert.deepEqual(detectConflicts(registry, {}).filter(issue => issue.commandIds.includes(command.id)), []);
});

test("全局设置和旧快捷键入口都走命令；Mod+, 只属于设置", () => {
  const calls: string[] = [];
  const registry = createCommandRegistry({
    openSettings: () => calls.push("settings"),
    openShortcuts: () => calls.push("shortcuts"),
  } as unknown as CommandDeps);
  const settings = registry.find((item) => item.id === "settings.open");
  const shortcuts = registry.find((item) => item.id === "help.shortcuts");
  assert.ok(settings);
  assert.ok(shortcuts);
  assert.equal(settings.menu, "file");
  assert.equal(settings.defaultKey, "Mod+,");
  assert.equal(shortcuts.defaultKey, undefined);
  settings.run();
  shortcuts.run();
  assert.deepEqual(calls, ["settings", "shortcuts"]);
  assert.deepEqual(detectConflicts(registry, {}).filter((issue) => issue.blocking && issue.commandIds.includes("settings.open")), []);
});

test("定位与设置按当前工作流库可达，离线不封死入口且不占默认键", () => {
  let selected = true, busy = false, calls = 0;
  const deps = { repository: { canReconnect: () => true, reconnect: () => {}, canSettings: () => selected,
    canLocate: () => selected && !busy, locate: () => { calls++; } }, openLibrarySettings: () => { calls++; } } as unknown as CommandDeps;
  const commands = createCommandRegistry(deps);
  for (const id of ["file.repositorySettings", "repository.locate"]) {
    const command = commands.find(item => item.id === id)!;
    assert.equal(command.enabled?.(), true); assert.equal(command.defaultKey, undefined);
    command.run(); assert.equal(command.menu, "file");
    assert.deepEqual(detectConflicts(commands, {}).filter(issue => issue.commandIds.includes(id)), []);
  }
  assert.equal(calls, 2); busy = true;
  assert.equal(commands.find(item => item.id === "repository.locate")!.enabled?.(), false);
  selected = false; assert.equal(commands.find(item => item.id === "file.repositorySettings")!.enabled?.(), false);
});

test("AI actions have explicit context, reuse callbacks and reserve no default hotkeys", () => {
  const calls: string[] = [];
  const deps = { flow: () => "browse", browse: { ai: { recognize: (rerun: boolean) => calls.push(rerun ? "again" : "recognize"), tasks: () => calls.push("tasks"), models: () => calls.push("models") } } } as unknown as CommandDeps;
  const commands = createCommandRegistry(deps);
  for (const id of ["ai.recognize", "ai.recognizeAgain", "ai.tasks", "ai.models"]) {
    const command = commands.find((item) => item.id === id)!;
    assert.ok(command); assert.equal(command.defaultKey, undefined); command.run();
  }
  assert.deepEqual(calls, ["recognize", "again", "tasks", "models"]);
});

test("AI 命令按实际构建能力隐藏，既有标签命令保持可用", () => {
 let available=false;
 const deps={flow:()=>"browse",browse:{ai:{available:()=>available,recognize:()=>{},tasks:()=>{},models:()=>{}}}} as unknown as CommandDeps;
 const commands=createCommandRegistry(deps).filter(c=>c.id.startsWith("ai."));
 assert.equal(commands.length,4);for(const c of commands){assert.equal(c.when?.(),false);assert.equal(c.defaultKey,undefined);}
 available=true;for(const c of commands)assert.equal(c.when?.(),true);
});
