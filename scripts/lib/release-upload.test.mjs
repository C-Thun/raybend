import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync, symlinkSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { finalizeRelease } from '../finalize-release.mjs';
import { githubReleasePlan } from './release-upload.mjs';

function fixture(channel = 'release') {
  const root = mkdtempSync(join(tmpdir(), "raybend-'中文-"));
  const input = join(root, 'bundle'), out = join(root, 'out'), manifest = join(root, 'manifest.json');
  mkdirSync(input);
  const name = 'RayBend 中文.EXE';
  writeFileSync(join(input, name), 'synthetic installer fixture only');
  writeFileSync(join(input, name + '.sig'), 'synthetic signature');
  writeFileSync(manifest, JSON.stringify({ schema: 1, version: channel === 'beta' ? '1.2.3-beta.1' : '1.2.3', channel, builtAt: '2026-09-27T00:00:00Z', gitHash: 'abcdef123456', dirty: false }));
  const argv = [input, '--manifest', manifest, '--out', out, '--allow-unsigned', '--base-url', `https://github.com/C-Thun/raybend/releases/download/v${channel === 'beta' ? '1.2.3-beta.1' : '1.2.3'}/`];
  return { root, input, out, manifest, name, argv, dispose: () => rmSync(root, { recursive: true, force: true }) };
}
test('最终目录含精确安装器与更新签名；只读计划含完整上传文件', () => {
  const f = fixture();
  try {
    finalizeRelease({ argv: f.argv, run: () => { throw new Error('不应执行外部命令'); }, log: () => {} });
    const plan = githubReleasePlan(f.out);
    assert.equal(plan.assets.length, 6);
    assert.equal(plan.tag, 'v1.2.3');
    assert.equal(readFileSync(join(f.out, f.name), 'utf8'), 'synthetic installer fixture only');
    assert.throws(() => finalizeRelease({ argv: f.argv, log: () => {} }), /已存在/);
  } finally { f.dispose(); }
});
test('预览包不会占用 latest；test、脏源与清单篡改不能上传', () => {
  const f = fixture('beta');
  try {
    finalizeRelease({ argv: f.argv, log: () => {} });
    assert.equal(githubReleasePlan(f.out).prerelease,true);
    const indexPath = join(f.out, 'release-index.json');
    const original = readFileSync(indexPath, 'utf8');
    for (const change of [{ dirty: true }, { channel: 'test' }, { gitHash: null }]) {
      writeFileSync(indexPath, JSON.stringify({ ...JSON.parse(original), ...change }));
      assert.throws(() => githubReleasePlan(f.out), /公开发行/);
    }
    writeFileSync(indexPath, original);
    writeFileSync(join(f.out, 'beta.json'), '{}');
    assert.throws(() => githubReleasePlan(f.out), /更新清单/);
  } finally { f.dispose(); }
});
test('变更字节、残留文件、缺签名、符号链接与重复 basename 均拒绝', () => {
  const f = fixture();
  try {
    finalizeRelease({ argv: f.argv, log: () => {} });
    writeFileSync(join(f.out, 'old.json'), '{}');
    assert.throws(() => githubReleasePlan(f.out), /未知文件/);
    rmSync(join(f.out, 'old.json'));
    writeFileSync(join(f.out, f.name), 'changed');
    assert.throws(() => githubReleasePlan(f.out), /已改变/);
    symlinkSync(f.input, join(f.out, 'link'));
    assert.throws(() => githubReleasePlan(f.out), /符号链接/);
    rmSync(join(f.input, f.name + '.sig'));
    assert.throws(() => finalizeRelease({ argv: f.argv.map(x => x === f.out ? join(f.root, 'missing-sig') : x), log: () => {} }), /缺少更新签名/);
    assert.equal(existsSync(join(f.root, 'missing-sig')), false);
    mkdirSync(join(f.input, 'duplicate'));
    writeFileSync(join(f.input, 'duplicate', f.name), 'duplicate');
    assert.throws(() => finalizeRelease({ argv: [f.input, '--manifest', f.manifest, '--out', join(f.root, 'dup'), '--allow-unsigned'], log: () => {} }), /重复/);
  } finally { f.dispose(); }
});
test('手动更新包无 JSON；签名验证失败不写输出；非法 CLI 不执行', () => {
  const f = fixture();
  try {
    assert.throws(() => finalizeRelease({ argv: f.argv.filter(x => x !== '--allow-unsigned'), run: () => { throw new Error('signtool failure'); }, log: () => {} }), /signtool/);
    assert.equal(existsSync(f.out), false);
    assert.throws(() => finalizeRelease({ argv: [f.input, '--manifest', '--out'], log: () => {} }), /非法/);
    finalizeRelease({ argv: f.argv.slice(0, -2), log: () => {} });
    assert.equal(existsSync(join(f.out, 'latest.json')), false);
    assert.equal(githubReleasePlan(f.out).assets.length, 4);
  } finally { f.dispose(); }
});

test('自动收口只收本版、目标完整；旧 Windows bundle 不混入',()=>{
 const f=fixture();try{
  rmSync(join(f.input,f.name));rmSync(join(f.input,f.name+'.sig'));
  writeFileSync(join(f.input,'RayBend_1.2.2_x64-setup.exe'),'old');
  writeFileSync(join(f.input,'RayBend_1.2.3_x64-setup.exe'),'current');
  assert.throws(()=>finalizeRelease({argv:f.argv.slice(0,-2),selectVersion:'1.2.3',requiredTargets:['nsis','msi'],log:()=>{}}),/msi/);
  assert.equal(existsSync(f.out),false);
  writeFileSync(join(f.input,'RayBend_1.2.3_x64_en-US.msi'),'current MSI');
  finalizeRelease({argv:f.argv.slice(0,-2),selectVersion:'1.2.3',requiredTargets:['nsis','msi'],log:()=>{}});
  assert.equal(existsSync(join(f.out,'RayBend_1.2.2_x64-setup.exe')),false);
  assert.equal(githubReleasePlan(f.out).assets.filter(p=>/\.(exe|msi)$/i.test(p)).length,2);
 }finally{f.dispose();}
});
