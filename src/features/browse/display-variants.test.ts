import test from 'node:test';import assert from 'node:assert/strict';
import {createBrowseDisplay} from './display-variants.ts';
import type {VariantSnapshot} from '../../lib/export-model.ts';
const snap=(assetId:number,variant:string):VariantSnapshot=>({reference:{assetId,variant},name:'柔光',profileHash:variant,relPath:'中文.jpg',sourceSignature:'original',stack:{values:{exposure:1}}});
const histogram={bins:1,r:[1],g:[1],b:[1],max:1};
const details=async()=>({width:400,height:800,histogram});
test('display switches neither latest nor catalog, is independent per asset, and resets by scope',async()=>{
 const calls:string[]=[];const display=createBrowseDisplay({snapshot:async(_,id,choice)=>{calls.push(choice);return snap(id,choice);},details});display.context('repo/a');
 await display.select('repo',1,'issue:7');await display.select('repo',2,'sooc');assert.equal(display.get(1)?.target.captured?.profileHash,'issue:7');
 await display.select('repo',1,'latest');assert.equal(display.get(1),undefined);assert.equal(display.get(2)?.choice,'sooc');assert.deepEqual(calls,['issue:7','sooc']);
 display.context('repo/a');assert.equal(display.choices().size,1);display.context('repo/b');assert.equal(display.choices().size,0);display.dispose();
});
test('late draft loads cannot restore selection after latest, context switch or disposal',async()=>{
 let finish!:(value:VariantSnapshot)=>void;const display=createBrowseDisplay({snapshot:()=>new Promise(resolve=>{finish=resolve;}),details});display.context('a');
 const pending=display.select('repo',1,'issue:7');await display.select('repo',1,'latest');finish(snap(1,'issue:7'));await pending;assert.equal(display.choices().size,0);
 const next=display.select('repo',1,'issue:7');display.context('b');finish(snap(1,'issue:7'));await next;assert.equal(display.choices().size,0);
 const last=display.select('repo',1,'sooc');display.dispose();finish(snap(1,'sooc'));await last;assert.equal(display.choices().size,0);
});
test('failed variant resolution preserves the last displayed issue',async()=>{
 let fail=false;const display=createBrowseDisplay({snapshot:async(_,id,choice)=>{if(fail)throw Error('deleted');return snap(id,choice);},details});await display.select('repo',1,'sooc');fail=true;
 await assert.rejects(display.select('repo',1,'issue:7'),/deleted/);assert.equal(display.get(1)?.choice,'sooc');
});
