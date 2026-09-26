import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { runRelease } from '../release.mjs';
function fixture(){
 const root=mkdtempSync(join(tmpdir(),'raybend-release-'));
 mkdirSync(join(root,'public/legal'),{recursive:true});writeFileSync(join(root,'public/legal/third-party.json'),'{}');
 mkdirSync(join(root,'src-tauri'));mkdirSync(join(root,'crates/raybend/src/raw'),{recursive:true});
 writeFileSync(join(root,'package.json'),JSON.stringify({version:'0.1.0',packageManager:'pnpm@12.3.4'}));
 writeFileSync(join(root,'Cargo.toml'),'[workspace.package]\nversion = "0.1.0"\n');
 writeFileSync(join(root,'Cargo.lock'),'[[package]]\nname = "raybend"\nversion = "0.1.0"\n\n[[package]]\nname = "raybend-desktop"\nversion = "0.1.0"\n');
 writeFileSync(join(root,'src-tauri/tauri.conf.json'),JSON.stringify({version:'../package.json'}));
 writeFileSync(join(root,'crates/raybend/src/raw/worker.rs'),'pub const PROTOCOL_TAG: &str = "fixture-v1";');
 return {root,dispose:()=>rmSync(root,{recursive:true,force:true})};
}
test('dry-run 完全不写入和构建；失败恢复三处版本且不覆盖其他字段',()=>{
 const f=fixture();try{
 const before=['package.json','Cargo.toml','Cargo.lock'].map(p=>readFileSync(join(f.root,p),'utf8'));
 const run=(cmd,args)=>{if(cmd==='git')return args[0]==='status'?'':'0123456789ab';throw new Error('build failed');};
 runRelease({root:f.root,argv:['minor','--dry-run'],run,log:()=>{}});
 assert.throws(()=>runRelease({root:f.root,argv:['minor'],run,log:()=>{}}),/已恢复/);
 ['package.json','Cargo.toml','Cargo.lock'].forEach((p,i)=>assert.equal(readFileSync(join(f.root,p),'utf8'),before[i]));
 }finally{f.dispose();}
});
test('自测产物独立、manifest 校验字节和协议；不改版本',()=>{
 const f=fixture();try{
 mkdirSync(join(f.root,'dist'));writeFileSync(join(f.root,'dist/index.html'),'existing production');
 const run=(cmd,args,opts)=>{if(cmd==='git')return args[0]==='status'?'M dirty':'abcdef012345';assert.equal(cmd,'pnpm');if(args[0]==='licenses:generate')return;assert.deepEqual(args,['build','--outDir','dist/test-build']);assert.equal(opts.env.RAYBEND_CHANNEL,'test');mkdirSync(join(f.root,'dist/test-build'));writeFileSync(join(f.root,'dist/test-build/index.html'),'test');};
 runRelease({root:f.root,argv:['test'],run,log:()=>{}});
 assert.equal(readFileSync(join(f.root,'dist/index.html'),'utf8'),'existing production');
 const manifest=JSON.parse(readFileSync(join(f.root,'dist/test-build/raybend-build.json'),'utf8'));
 assert.equal(manifest.workerProtocol,'fixture-v1');assert.equal(manifest.dirty,true);assert.equal(manifest.files[0].bytes,4);assert.match(manifest.files[0].sha256,/^[a-f0-9]{64}$/);assert.equal(existsSync(join(f.root,'.release')),false);
 assert.equal(JSON.parse(readFileSync(join(f.root,'package.json'),'utf8')).version,'0.1.0');
 }finally{f.dispose();}
});

test('第一条 Windows 指令构建一次、保存源码快照并自动调用 finalize；自测输出不覆盖正式输出',()=>{
 const f=fixture();try{
 let finalized;
 const calls=[];
 const run=(cmd,args,opts)=>{
  calls.push([cmd,...args]);
  if(cmd==='git')return args[0]==='status'?'':'abcdef012345';
  if(cmd==='wslpath')return 'C:\\synthetic';
  if(cmd==='cmd.exe')return 'tauri-cli 2.11.4';
  assert.equal(cmd,'pnpm');
  if(args[0]==='licenses:generate'||args[0]==='check:win')return;
  assert.deepEqual(args,['build','--outDir','dist/test-build']);
  mkdirSync(join(f.root,'dist/test-build'),{recursive:true});writeFileSync(join(f.root,'dist/test-build/index.html'),'synthetic');
 };
 runRelease({root:f.root,argv:['test','--windows','--unsigned'],run,log:()=>{},finalize:opts=>{finalized=opts;}});
 assert.equal(calls.filter(c=>c[0]==='pnpm'&&c[1]==='build').length,1);
 assert.deepEqual(finalized.requiredTargets,['nsis']);
 assert.equal(finalized.selectVersion,'0.1.0');
 assert.ok(finalized.argv.includes('--allow-unsigned'));
 const manifest=JSON.parse(readFileSync(join(f.root,'dist/test-build/raybend-build.json'),'utf8'));
 assert.equal(Object.keys(manifest.sourceFiles).length,4);
 assert.match(manifest.sourceFiles['Cargo.lock'],/^[a-f0-9]{64}$/);
 }finally{f.dispose();}
});

test('缺 Windows CLI 在任何版本写入/前端构建前报告，不污染已有工作树',()=>{
 const f=fixture();try{
 const before=readFileSync(join(f.root,'package.json'),'utf8');
 const run=(cmd,args)=>{if(cmd==='git')return args[0]==='status'?'':'abcdef012345';if(cmd==='cmd.exe')throw new Error('missing CLI');throw new Error('unexpected build');};
 assert.throws(()=>runRelease({root:f.root,argv:['patch','--windows','--unsigned'],run,log:()=>{}}),/cargo install tauri-cli/);
 assert.equal(readFileSync(join(f.root,'package.json'),'utf8'),before);
 }finally{f.dispose();}
});
