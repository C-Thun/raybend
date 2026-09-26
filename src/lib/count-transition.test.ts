import assert from 'node:assert/strict';
import test from 'node:test';
import { animateCount, countTransition } from './count-transition.ts';

test('queue counts reach exact integer targets monotonically within 3s and at most 10Hz', () => {
  for (const [from,to] of [[0,0],[0,1],[5,7],[0,9],[0,10],[30,130],[0,100000],[0,Number.MAX_SAFE_INTEGER],[200,0]]) {
    const frames = countTransition(from!,to!);
    assert.equal(frames.length === 0, from === to);
    let delay = 0, value = from!;
    for (const frame of frames) {
      assert(frame.delay-delay >= 100);
      assert(Number.isSafeInteger(frame.value));
      assert(to! > from! ? frame.value > value : frame.value < value);
      assert(frame.value >= Math.min(from!,to!) && frame.value <= Math.max(from!,to!));
      delay=frame.delay; value=frame.value;
    }
    assert.equal(value,to);
    assert(delay <= 3000); assert(frames.length <= 30);
    if (Math.abs(to!-from!)>=10) assert(delay>=1000);
  }
  assert.deepEqual(countTransition(8,9),[{delay:200,value:9}]);
  assert(countTransition(0,9).slice(-1)[0]!.delay<1000);
  for (const value of [-1,1.5,Infinity,NaN,Number.MAX_SAFE_INTEGER+1]) {
    assert.throws(()=>countTransition(value,1),RangeError);
    assert.throws(()=>countTransition(1,value),RangeError);
  }
});

test('suspended windows catch up once and cancelled counters never emit stale values', () => {
  let now=0, next=0;
  const pending=new Map<number,()=>void>();
  const clock={now:()=>now,set:(fn:()=>void,_delay:number)=>{pending.set(++next,fn);return next as unknown as ReturnType<typeof setTimeout>;},clear:(id:ReturnType<typeof setTimeout>)=>{pending.delete(id as unknown as number);}};
  const values:number[]=[];
  const cancel=animateCount(0,1000,v=>values.push(v),clock);
  assert.equal(pending.size,1);
  now=3000;const fn=[...pending.values()][0]!;pending.clear();fn();
  assert.deepEqual(values,[1000]);assert.equal(pending.size,0);
  cancel();
  const stop=animateCount(0,10,v=>values.push(v),clock);
  const stale=[...pending.values()][0]!;stop();assert.equal(pending.size,0);stale();
  assert.deepEqual(values,[1000]);
  animateCount(2,2,v=>values.push(v),clock);assert.deepEqual(values,[1000,2]);
});
