/** Repeatable DOM regression, using the existing browse CDP/mock-backend harness. */
import assert from "node:assert/strict";

export async function checkBrowseStability(send) {
  const evaluate = async (expression) => {
    const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result?.value;
  };
  const frames = () => evaluate(`new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))`);
  const waitFor = async (expression) => {
    for (let i = 0; i < 100; i++) {
      if (await evaluate(expression)) return;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    throw new Error("Timed out: " + expression);
  };
  await waitFor(`document.querySelectorAll('main [data-virtual-scroller] [role="option"]').length > 2`);
  await evaluate(`(() => {
    const s = document.querySelector('main [data-virtual-scroller]');
    window.__SCROLLER = s;
    s.style.maxHeight = '310px';
    s.scrollTop = s.scrollHeight * 0.65;
    s.dispatchEvent(new Event('scroll'));
  })()`);
  await waitFor(`(window.__INVOKE_LOG || []).filter(cmd=>cmd==='browse_page').length>=2`);
  await frames();
  await waitFor(`(() => {
    const s = window.__SCROLLER;
    return s.scrollTop > 1000 && [...s.querySelectorAll('[role="option"]')].some(t => /MY00/.test(t.textContent));
  })()`);
  // Click partially visible as well as fully visible rows; every click must keep
  // the same DOM scroller and exact offset, including after page/metadata replies.
  for (const edge of ["top", "middle", "bottom"]) {
    const before = await evaluate(`(() => {
      const s=window.__SCROLLER, box=s.getBoundingClientRect();
      const tiles=[...s.querySelectorAll('[role="option"]')].filter(t=>{
        const r=t.getBoundingClientRect(); return r.bottom>box.top && r.top<box.bottom;
      });
      const index=${JSON.stringify(edge)}==='top'?0:${JSON.stringify(edge)}==='bottom'?tiles.length-1:Math.floor(tiles.length/2);
      const tile=tiles[index]; if(!tile)throw Error('no visible tile');
      const top=s.scrollTop;
      tile.dispatchEvent(new MouseEvent('click',{bubbles:true}));
      return top;
    })()`);
    await frames();
    assert.equal(await evaluate(`window.__SCROLLER === document.querySelector('main [data-virtual-scroller]')`), true);
    assert.equal(await evaluate(`window.__SCROLLER.scrollTop`), before, `${edge} tile moved the grid`);
  }
  await waitFor(`document.querySelector('[data-browse-info]')?.getAttribute('aria-busy')==='false'`);
  // Hold both requests, complete only EXIF first, then issues: no partial panel.
  async function checkInfoSwap(container) {
    const old = await evaluate(`(() => {
    window.__DELAY_INFO=true;
    const panel=document.querySelector('[data-browse-info]');
    window.__INFO_PANEL=panel;
    const old=panel.textContent;
    const tile=[...document.querySelector(${JSON.stringify(container)}).querySelectorAll('[role="option"], [data-strip-item]')].find(t=>t.getAttribute('aria-selected')!=='true' && t.getAttribute('data-current')!=='true');
    tile.dispatchEvent(new MouseEvent('click',{bubbles:true}));
    return old;
  })()`);
  await waitFor(`(window.__INFO_PENDING?.length ?? 0)>=2`);
  assert.equal(await evaluate(`window.__INFO_PANEL.textContent`), old);
  assert.equal(await evaluate(`window.__INFO_PANEL.querySelector('select')?.disabled`), true);
  await evaluate(`window.__INFO_PENDING.filter(p=>p.cmd==='file_exif').forEach(p=>p.finish())`);
  await frames();
  assert.equal(await evaluate(`window.__INFO_PANEL.textContent`), old, "EXIF alone must not replace the panel");
  await evaluate(`window.__DELAY_INFO=false; window.__INFO_PENDING.forEach(p=>p.finish()); window.__INFO_PENDING=[];`);
  await waitFor(`window.__INFO_PANEL.getAttribute('aria-busy')==='false'`);
  assert.notEqual(await evaluate(`window.__INFO_PANEL.textContent`), old);
  assert.equal(await evaluate(`window.__INFO_PANEL===document.querySelector('[data-browse-info]')`), true);
  }
  await checkInfoSwap('main [data-virtual-scroller]');
  // Keyboard navigation must still scroll to an off-screen neighbour.
  await evaluate(`(() => {
    const s=window.__SCROLLER;
    const tiles=[...s.querySelectorAll('[role="option"]')];
    tiles.at(-1).dispatchEvent(new MouseEvent('click',{bubbles:true}));
    window.__KEY_SCROLL=s.scrollTop;
    window.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));
  })()`);
  await frames();
  assert.ok(await evaluate(`window.__SCROLLER.scrollTop>window.__KEY_SCROLL`), "keyboard focus must scroll");
  await evaluate(`window.__SCROLLER.querySelector('[aria-selected="true"]').dispatchEvent(new MouseEvent('dblclick',{bubbles:true}))`);
  await waitFor(`document.querySelector('[data-strip-item]') && document.querySelector('[data-browse-info]')?.getAttribute('aria-busy')==='false'`);
  await checkInfoSwap('[data-filmstrip]');
  console.log("✓ browse: 520 photos across data pages, scrolling clicks preserve DOM/offset; tiles/film right panel swaps after both reads; keyboard still scrolls");
}
