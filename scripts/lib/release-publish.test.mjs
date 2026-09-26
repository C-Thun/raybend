import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { finalizeRelease } from '../finalize-release.mjs';
import { publishRelease, RELEASE_SOURCE_FILES } from './release-publish.mjs';
import { versionEdits, sha256 } from './release-files.mjs';

function fixture(licensePadding=0) {
  const root = mkdtempSync(join(tmpdir(), 'raybend-publish-fixture-'));
  mkdirSync(join(root, 'public/legal'), { recursive: true });
  mkdirSync(join(root, 'bundle'));
  const original = {
    'package.json': '{"version":"1.2.2"}\n',
    'Cargo.toml': '[workspace.package]\nversion = "1.2.2"\n',
    'Cargo.lock': '[[package]]\nname = "raybend"\nversion = "1.2.2"\n\n[[package]]\nname = "raybend-desktop"\nversion = "1.2.2"\n',
  };
  for (const [path, text] of Object.entries(original)) writeFileSync(join(root, path), text);
  versionEdits(root, '1.2.3').forEach(e => writeFileSync(e.path, e.after));
  writeFileSync(join(root, 'pnpm-lock.yaml'), 'synthetic lock');
  writeFileSync(join(root, 'public/legal/third-party.json'), JSON.stringify({ fixturePadding: 'x'.repeat(licensePadding), locks: { cargo: sha256(readFileSync(join(root, 'Cargo.lock'))), pnpm: sha256(readFileSync(join(root, 'pnpm-lock.yaml'))) } }));
  const base = 'a'.repeat(40), releaseHead = 'b'.repeat(40);
  const manifest = { schema: 1, channel: 'release', version: '1.2.3', builtAt: '2026-09-27T00:00:00Z', dirty: false, gitHash: base, sourceFiles: Object.fromEntries(RELEASE_SOURCE_FILES.map(p => [p, sha256(readFileSync(join(root, p)))])) };
  writeFileSync(join(root, 'manifest.json'), JSON.stringify(manifest));
  writeFileSync(join(root, 'bundle/RayBend_1.2.3_x64-setup.exe'), 'synthetic bytes, not an installer');
  const out = join(root, 'out');
  finalizeRelease({ argv: [join(root, 'bundle'), '--manifest', join(root, 'manifest.json'), '--out', out, '--allow-unsigned'], log: () => {} });
  const state = { head: base, paths: [...RELEASE_SOURCE_FILES], staged: false, tag: null, release: null, calls: [], partialOnce: false, siteFailed: false, previousSiteFailure:false, placeholderOnce:false, authFailed: false };
  const run = (cmd, args, options) => {
    state.calls.push([cmd, ...args]);
    if (cmd === 'git') {
      if (args[0] === 'rev-parse') return args[1].includes('^') ? (args[1].startsWith('v') ? state.tag : base) : state.head;
      if (args[0] === 'symbolic-ref') return 'master';
      if (args[0] === 'remote') return 'git@github.com:C-Thun/raybend.git';
      if (args[0] === 'rev-list') return `${state.head} ${base}`;
      if (args[0] === 'show') {
        const [ref, path] = args[1].split(':');
        if(!ref && readFileSync(join(root,path)).length>(options.maxBuffer??1024*1024))throw new Error('synthetic exec maxBuffer exceeded');
        return ref ? original[path] : (options.encoding === null ? readFileSync(join(root, path)) : readFileSync(join(root, path), 'utf8'));
      }
      if (args[0] === 'status') return state.paths.map(p => ` M ${p}\0`).join('');
      if (args[0] === 'diff') return state.staged ? 'somebody-else.txt' : '';
      if (args[0] === 'add') { assert.deepEqual(args.slice(2), RELEASE_SOURCE_FILES); return ''; }
      if (args[0] === 'commit') { state.head = releaseHead; state.paths = []; return ''; }
      if (args[0] === 'tag') { if (args[1] === '--list') return state.tag ? 'v1.2.3' : ''; state.tag = state.head; return ''; }
      if (args[0] === 'push') { assert.deepEqual(args, ['push', '--atomic', 'origin', 'HEAD:refs/heads/master', 'refs/tags/v1.2.3:refs/tags/v1.2.3']); return ''; }
    }
    if (cmd === 'gh') {
      if (args[0] === 'auth') { if(state.authFailed)throw new Error('auth failed'); return ''; }
      if (args[0] === 'api') {
        if (!state.release) { const e = new Error('not found'); e.stderr = 'HTTP 404'; throw e; }
        return JSON.stringify(state.release);
      }
      if (args[0] === 'release' && args[1] === 'create') { state.release = { tag_name: 'v1.2.3', prerelease: false, draft: true, assets: [], published_at: '2026-09-27T00:00:00Z' }; return ''; }
      if (args[0] === 'release' && args[1] === 'upload') {
        for (const path of args.slice(3, args.indexOf('--repo'))) {
          const bytes = readFileSync(path);
          assert.ok(!state.release.assets.some(a => a.name === relative(out, path)));
          if(state.placeholderOnce){state.placeholderOnce=false;state.release.assets.push({name:relative(out,path),size:0,state:'starter'});throw new Error('synthetic incomplete upload');}
          state.release.assets.push({ name: relative(out, path), size: bytes.length, digest: 'sha256:' + sha256(bytes) });
          if (state.partialOnce) { state.partialOnce = false; throw new Error('synthetic network interrupted'); }
        }
        return '';
      }
      if (args[0] === 'release' && args[1] === 'delete-asset') {assert.equal(state.release.draft,true);assert.equal(state.release.assets.find(a=>a.name===args[3]).state,'starter');state.release.assets=state.release.assets.filter(a=>a.name!==args[3]);return '';}
      if (args[0] === 'release' && args[1] === 'edit') { state.release.draft = false; return ''; }
      if (args[0] === 'run' && args[1] === 'list') return JSON.stringify([{ databaseId: 42, headSha: state.head, createdAt: '2026-09-27T00:00:01Z',status:state.previousSiteFailure?'completed':'queued',conclusion:state.previousSiteFailure?'failure':null }]);
      if (args[0] === 'run' && args[1] === 'rerun') {state.previousSiteFailure=false;return '';}
      if (args[0] === 'run' && args[1] === 'watch') { if (state.siteFailed) {state.previousSiteFailure=true;throw new Error('Pages failed');} return ''; }
    }
    throw new Error(`unexpected synthetic command: ${cmd} ${args}`);
  };
  return { root, out, state, run, call: execute => publishRelease({ root, directory: out, execute, run, log: () => {}, wait: () => {} }), dispose: () => rmSync(root, { recursive: true, force: true }) };
}
test('默认只读；显式执行顺序为精确提交/tag/原子推送/草稿完整上传/公开/等待官网', () => {
  const f = fixture();
  try {
    f.call(false);
    assert.ok(f.state.calls.every(c => c[0] === 'git' && !['add', 'commit', 'tag', 'push'].includes(c[1])));
    f.state.calls = [];
    f.call(true);
    assert.equal(f.state.release.draft, false);
    assert.equal(f.state.release.assets.length, 4);
    assert.ok(f.state.calls.findIndex(c => c[1] === 'push') < f.state.calls.findIndex(c => c[2] === 'create'));
    assert.ok(f.state.calls.findIndex(c => c[2] === 'upload') < f.state.calls.findIndex(c => c[2] === 'edit'));
    assert.equal(f.state.calls.at(-1)[2], 'watch');
    f.state.calls = [];
    f.call(true);
    assert.ok(!f.state.calls.some(c => c[1] === 'commit' || c[2] === 'upload' || c[2] === 'edit'));
  } finally { f.dispose(); }
});
test('并行代码/暂存/来源变更与未登录均不产生发布副作用', () => {
  for (const mutation of [f => f.state.paths.push('src/parallel.ts'), f => { f.state.staged = true; }, f => writeFileSync(join(f.root, 'package.json'), '{"version":"2.0.0"}'), f => { f.state.authFailed = true; }]) {
    const f = fixture();
    try {
      mutation(f);
      assert.throws(() => f.call(true));
      assert.ok(!f.state.calls.some(c => ['add', 'commit', 'tag', 'push'].includes(c[1]) || c[2] === 'create'));
    } finally { f.dispose(); }
  }
});
test('部分上传失败留草稿；同一指令只补缺失文件，不覆盖资产', () => {
  const f = fixture();
  try {
    f.state.partialOnce = true;
    assert.throws(() => f.call(true), /interrupted/);
    assert.equal(f.state.release.draft, true);
    assert.equal(f.state.release.assets.length, 1);
    f.call(true);
    assert.equal(f.state.release.draft, false);
    assert.equal(f.state.release.assets.length, 4);
    assert.equal(f.state.calls.filter(c => c[1] === 'commit').length, 1);
    assert.ok(!f.state.calls.some(c => c.includes('--clobber') || c.includes('--force')));
  } finally { f.dispose(); }
});
test('网站失败如实报错，重试不重传；已有远程文件冲突不覆盖', () => {
  const f = fixture();
  try {
    f.state.siteFailed = true;
    assert.throws(() => f.call(true), /Pages failed/);
    assert.equal(f.state.release.draft, false);
    f.state.siteFailed = false;
    f.call(true);
    assert.equal(f.state.calls.filter(c=>c[1]==='run'&&c[2]==='rerun').length,1);
    f.state.release.assets[0].digest = 'sha256:' + '0'.repeat(64);
    assert.throws(() => f.call(true), /远程资产/);
    assert.equal(f.state.calls.filter(c => c[2] === 'upload').length, 1);
  } finally { f.dispose(); }
});

test('草稿中断上传的 starter 零字节占位可移除重传，已完成资产不删除',()=>{
 const f=fixture();try{
 f.state.placeholderOnce=true;
 assert.throws(()=>f.call(true),/incomplete upload/);
 assert.equal(f.state.release.assets[0].state,'starter');
 f.call(true);
 assert.equal(f.state.release.draft,false);
 assert.equal(f.state.release.assets.length,4);
 assert.equal(f.state.calls.filter(c=>c[2]==='delete-asset').length,1);
 }finally{f.dispose();}
});

test('真实清单量级超过 exec 默认 1MiB 时仍能核对并发布，不截断许可',()=>{
 const f=fixture(1_100_000);try{f.call(true);assert.equal(f.state.release.draft,false);}finally{f.dispose();}
});
