/** Integer queue counters: <=10 updates/s, exact target, <=3 seconds. */
export function countTransition(from: number, to: number): readonly { delay: number; value: number }[] {
  if (![from,to].every(n=>Number.isSafeInteger(n)&&n>=0)) throw new RangeError('Invalid queue count');
  const distance=Math.abs(to-from);
  if (!distance) return [];
  const duration=distance<10 ? Math.max(200,distance*100) : Math.min(3000,Math.ceil((1000+Math.log10(distance/10)*1000)/100)*100);
  const steps=Math.min(distance,Math.floor(duration/100));
  const sign=to>from?1:-1;
  return Array.from({length:steps},(_,i)=>({delay:Math.round(duration*(i+1)/steps),value:i===steps-1?to:from+sign*Math.floor(distance*((i+1)/steps))}));
}

/** One timer; a suspended window catches up once instead of bursting old frames. */
export function animateCount(from:number,to:number,update:(value:number)=>void,
  clock={now:()=>performance.now(),set:(fn:()=>void,delay:number)=>setTimeout(fn,delay),clear:(id:ReturnType<typeof setTimeout>)=>clearTimeout(id)}) : () => void {
  const frames=countTransition(from,to);
  if(!frames.length){update(to);return ()=>{};}
  const start=clock.now();let index=0,cancelled=false;
  let timer:ReturnType<typeof setTimeout>;
  const tick=()=>{
    if(cancelled)return;
    const elapsed=clock.now()-start;
    while(index+1<frames.length && frames[index+1]!.delay<=elapsed)index++;
    update(frames[index]!.value);index++;
    if(index<frames.length)timer=clock.set(tick,Math.max(100,frames[index]!.delay-elapsed));
  };
  timer=clock.set(tick,frames[0]!.delay);
  return ()=>{cancelled=true;clock.clear(timer);};
}
