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
