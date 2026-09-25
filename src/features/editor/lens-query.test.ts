import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createLensQuery} from './lens-query.ts';
import type {LensMatch} from '../../api/types.ts';
const data = (name: string): LensMatch => ({ready:true, detected:null, lensName:name, focalMm:12, candidates:[], warnings:[]});

test('opening/retrying requests fresh candidates; selecting a photo alone never matches or applies', async () => {
  let calls = 0;
  const query = createLensQuery(async () => { calls++; return data('lens'); });
  query.select('repo', 1);
  assert.equal(calls, 0);
  assert.equal(query.state().status, 'idle');
  await query.refresh();
  assert.equal(query.state().status, 'ready');
  await query.refresh();
  assert.equal(calls, 2);
  query.dispose();
});

test('failures preserve their reason and retry replaces error with ready, including unmatched photos', async () => {
  let fail = true;
  const query = createLensQuery(async () => { if (fail) throw new Error('offline'); return data('unknown'); });
  query.select('repo', 1);
  assert.equal(await query.refresh(), null);
  assert.equal(query.state().status, 'error');
  assert.match(query.state().error!, /offline/);
  fail = false;
  await query.refresh();
  assert.equal(query.state().status, 'ready');
  assert.equal(query.state().data?.detected, null);
  assert.equal(query.state().error, null);
  query.dispose();
});

test('concurrent refreshes coalesce and a late response cannot replace a different asset', async () => {
  let complete!: (value:LensMatch)=>void;
  const query = createLensQuery(() => new Promise(resolve => {complete=resolve;}));
  query.select('repo', 1);
  const old = query.refresh();
  assert.equal(query.refresh(), old);
  query.select('repo', 2);
  complete(data('old'));
  assert.equal(await old, null);
  assert.equal(query.state().status, 'idle');
  assert.equal(query.state().data, null);
  query.dispose();
});

test('a stalled request becomes retryable timeout instead of loading forever', async () => {
  const query = createLensQuery(() => new Promise(() => {}), 5);
  query.select('repo', 1);
  await query.refresh();
  assert.equal(query.state().status, 'error');
  assert.equal(query.state().failure, 'timeout');
  query.dispose();
});

test('null runtime / unready result is unavailable, never a permanent loading state', async () => {
  for (const response of [null, {...data('lens'), ready:false}]) {
    const query = createLensQuery(async () => response);
    query.select('repo', 1);
    await query.refresh();
    assert.equal(query.state().failure, 'unavailable');
    query.dispose();
  }
});

test('refresh failure retains usable candidates while switching photos clears them', async () => {
  let fail = false;
  const query = createLensQuery(async () => { if (fail) throw new Error('worker unavailable'); return data('known'); });
  assert.equal(await query.refresh(), null);
  query.select('repo', 1);
  await query.refresh();
  fail = true;
  await query.refresh();
  assert.equal(query.state().status, 'error');
  assert.equal(query.state().data?.lensName, 'known');
  query.select('repo', 2);
  assert.equal(query.state().data, null);
  query.dispose();
});

test('an old request cannot clear or overwrite a newer pending request, even when returning to the same photo', async () => {
  const replies: ((value: LensMatch) => void)[] = [];
  const query = createLensQuery(() => new Promise(resolve => { replies.push(resolve); }));
  query.select('repo', 1);
  const old = query.refresh();
  query.select('repo', 2);
  query.select('repo', 1);
  const current = query.refresh();
  assert.equal(await old, null);
  replies[0]!(data('old'));
  assert.equal(query.refresh(), current);
  assert.equal(query.state().status, 'loading');
  replies[1]!(data('new'));
  assert.equal((await current)?.lensName, 'new');
  assert.equal(query.state().data?.lensName, 'new');
  query.dispose();
});
