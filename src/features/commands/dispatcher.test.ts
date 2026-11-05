import assert from "node:assert/strict";
import test from "node:test";
import {createCommandDispatcher} from "./dispatcher.ts";
import type {CommandSpec} from "../../lib/commands.ts";
function event(key:string, tagName="DIV") {
 return {key,target:{tagName,isContentEditable:false},preventDefault(this:{defaultPrevented:boolean}){this.defaultPrevented=true;},defaultPrevented:false} as unknown as KeyboardEvent;
}
test("modal commands require an explicit allowed subset; input keys stay local",()=>{
 const calls:string[]=[];
 const commands:CommandSpec[]=[{id:"remove",titleKey:"cmd.export.remove",group:"edit",scope:"tiles",defaultKey:"Delete",run:()=>{calls.push("remove");}}];
 let blocked=true,allowed=false;
 const dispatcher=createCommandDispatcher({commands:()=>commands,overrides:()=>({}),blocked:()=>blocked,allowedWhileBlocked:()=>allowed,platform:()=>"win"});
 assert(!dispatcher.handle(event("Delete")));allowed=true;assert(dispatcher.handle(event("Delete")));
 assert(!dispatcher.handle(event("Delete","INPUT")));blocked=false;allowed=false;assert(dispatcher.handle(event("Delete")));
 assert.deepEqual(calls,["remove","remove"]);
});

/*
 * 回归（崔总 2026-11-05）：编辑区 `Space` 那条命令**从上线起一次都跑不通** ——
 * 真实空格键的 `event.key` 是 `" "`，`normalizeKey` 把它 trim 成空串。
 * 这里用**真实形状的事件**（不是 `key: "Space"`）守住分发链路。
 */
test("真实空格键能跑到 edit 场景的 editor.area.toggleFocus", () => {
 const calls:string[]=[];
 const commands:CommandSpec[]=[{id:"editor.area.toggleFocus",titleKey:"cmd.editor.areaFocus",group:"view",scope:"viewer",defaultKey:"Space",scenes:["edit"],when:()=>true,run:()=>{calls.push("toggle");}}];
 const dispatcher=createCommandDispatcher({commands:()=>commands,overrides:()=>({}),platform:()=>"win"});
 assert.equal(dispatcher.handle(event(" ")),true,"空格必须被吃掉（否则会触发按钮默认行为/滚动）");
 assert.equal(dispatcher.handle(event(" ","INPUT")),false,"输入框里打空格仍归输入框");
 assert.deepEqual(calls,["toggle"]);
});
