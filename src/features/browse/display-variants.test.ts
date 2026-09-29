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
test('tiles switch on snapshot; details enrich aspect and histogram later (崔总 2026-09-29)',async()=>{
 let finish!:(value:{width:number;height:number;histogram:typeof histogram})=>void;
 const display=createBrowseDisplay({snapshot:async(_,id,choice)=>snap(id,choice),
   details:()=>new Promise(resolve=>{finish=resolve;})});display.context('a');
 await display.select('repo',1,'issue:7'); // snapshot（毫秒级）已挂上，select 不等 details
 const phase1=display.get(1)!;
 assert.equal(phase1.choice,'issue:7');assert.ok(phase1.target.captured,'快照已在：tile 立刻可换 imageKey');
 assert.equal(phase1.natural,undefined,'宽高还没回来：aspect 退回基准');
 assert.equal(phase1.histogram.bins,0,'空直方图：不回退成基准照片的直方图');
 const base={aspectOf:()=>1.5} as never as import('../../components/ui/tiles/source.ts').TilesSource;
 const stub=display.adapt(base);
 assert.equal(stub.aspectOf('1'),1.5,'details 前：基准 aspect');
 finish({width:400,height:800,histogram});await new Promise(r=>setTimeout(r,0));
 const phase2=display.get(1)!;
 assert.deepEqual(phase2.natural,{width:400,height:800});assert.equal(phase2.histogram,histogram);
 assert.ok(stub.aspectOf('1')>0&&Math.abs(stub.aspectOf('1')-0.5)<0.01,'details 后：变体宽高（400×800）');
});
test('details failure keeps the switched choice (thumbnail usable, histogram empty)',async()=>{
 const display=createBrowseDisplay({snapshot:async(_,id,choice)=>snap(id,choice),details:async()=>{throw Error('render');}});
 await display.select('repo',1,'sooc');await new Promise(r=>setTimeout(r,0));
 assert.equal(display.get(1)?.choice,'sooc','渲染失败不回滚：缩略图已能用');
 assert.equal(display.get(1)?.histogram.bins,0);
});
