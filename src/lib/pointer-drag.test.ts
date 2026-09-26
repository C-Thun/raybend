import assert from 'node:assert/strict';
import test from 'node:test';
import { trackPointerDrag } from './pointer-drag.ts';

function fixture(threshold=6,capture=false) {
  const events=new EventTarget(),frames=new Map<number,FrameRequestCallback>();let frame=0;
  const seen:unknown[]=[];
  const host={setPointerCapture:(id:number)=>seen.push(['capture',id]),releasePointerCapture:(id:number)=>seen.push(['release',id])};
  const event={pointerId:1,clientX:10,clientY:20,currentTarget:host} as unknown as PointerEvent;
  const cancel=trackPointerDrag(event,{threshold,capture,start:p=>seen.push(['start',p]),move:p=>seen.push(['move',p]),end:(p,c,s)=>seen.push(['end',p,c,s])},
    {events,requestFrame:fn=>{frames.set(++frame,fn);return frame;},cancelFrame:id=>{frames.delete(id);}});
  const emit=(type:string,x=10,y=20,pointerId=1)=>events.dispatchEvent(Object.assign(new Event(type),{pointerId,clientX:x,clientY:y}));
  const flush=()=>{const pending=[...frames.values()];frames.clear();pending.forEach(fn=>fn(0));};
  return {seen,cancel,emit,flush,frames};
}
test('drag waits for threshold, ignores other pointers and emits latest position once per frame',()=>{
  const f=fixture();f.emit('pointermove',100,100,2);f.emit('pointermove',13,24);assert.deepEqual(f.seen,[]);
  f.emit('pointermove',16,20);f.emit('pointermove',25,30);f.emit('pointermove',35,40);
  assert.equal(f.frames.size,1);f.flush();assert.deepEqual(f.seen,[['start',{x:16,y:20}],['move',{x:35,y:40}]]);
  f.emit('pointerup',100,200,2);assert.equal(f.seen.length,2);
  f.emit('pointerup',40,50);assert.deepEqual(f.seen.slice(-2),[['move',{x:40,y:50}],['end',{x:40,y:50},false,true]]);
  f.emit('pointermove',90,90);f.cancel();assert.equal(f.seen.length,4);
});
test('clicks do not start drags and cancellation releases capture, listeners and pending frames',()=>{
  const click=fixture();click.emit('pointerup');assert.deepEqual(click.seen,[['end',{x:10,y:20},false,false]]);
  for (const reason of ['pointercancel','blur','dispose']) {
    const f=fixture(0,true);f.emit('pointermove',30,40);assert.equal(f.frames.size,1);
    if(reason==='dispose')f.cancel();else f.emit(reason);
    assert.equal(f.frames.size,0);assert.deepEqual(f.seen,[['capture',1],['start',{x:10,y:20}],['release',1],['end',{x:30,y:40},true,true]]);
    f.flush();f.emit('pointerup');assert.equal(f.seen.length,4);
  }
});
