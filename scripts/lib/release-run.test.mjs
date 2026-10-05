import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { runRelease } from '../release.mjs';
function fixture(){
 const base=mkdtempSync(join(tmpdir(),'raybend-release-'));
 const root=join(base,'repo'),mirror=join(base,'mirror'),target=join(base,'target');mkdirSync(root);
 mkdirSync(join(root,'src/api'),{recursive:true});writeFileSync(join(root,'src/api/dto-contract.json'),'{}');
 for(const name of ['rust-toolchain.toml','LICENSE','THIRD-PARTY-NOTICES.md'])writeFileSync(join(root,name),'fixture');
 mkdirSync(join(root,'public/legal'),{recursive:true});writeFileSync(join(root,'public/legal/third-party.json'),'{}');
 mkdirSync(join(root,'src-tauri'));mkdirSync(join(root,'crates/raybend/src/raw'),{recursive:true});
 writeFileSync(join(root,'package.json'),JSON.stringify({version:'0.1.0',packageManager:'pnpm@12.3.4'}));
 writeFileSync(join(root,'Cargo.toml'),'[workspace.package]\nversion = "0.1.0"\n');
 writeFileSync(join(root,'Cargo.lock'),'[[package]]\nname = "raybend"\nversion = "0.1.0"\n\n[[package]]\nname = "raybend-desktop"\nversion = "0.1.0"\n');
 writeFileSync(join(root,'src-tauri/tauri.conf.json'),JSON.stringify({version:'../package.json'}));
 writeFileSync(join(root,'crates/raybend/src/raw/worker.rs'),'pub const PROTOCOL_TAG: &str = "fixture-v1";');
 const paths={hostBase:base,hostDrive:base,hostSource:mirror,hostTarget:target,windowsBase:'D:\\构建 目录',windowsSource:'D:\\构建 目录\\source',windowsTarget:'D:\\构建 目录\\target'};
 return {root,mirror,target,paths,dispose:()=>rmSync(base,{recursive:true,force:true})};
}
test('dry-run 完全不写入和构建；失败恢复三处版本且不覆盖其他字段',()=>{
 const f=fixture();try{
 const before=['package.json','Cargo.toml','Cargo.lock'].map(p=>readFileSync(join(f.root,p),'utf8'));
 const run=(cmd,args)=>{if(cmd==='git')return args[0]==='status'?'':'0123456789ab';throw new Error('build failed');};
 runRelease({root:f.root,checkAssets:()=>{},discover:()=>f.paths,argv:['minor','--dry-run'],run,log:()=>{}});
 for(const targets of [['--win-msi'],['--win-nsis'],['--win-msi','--win-nsis']]) {
  const plan=runRelease({root:f.root,checkAssets:()=>{},discover:()=>f.paths,argv:['patch',...targets,'--unsigned','--dry-run'],run,log:()=>{}});
  assert.equal(plan.windowsTargets.length,targets.length);
 }
 assert.equal(existsSync(f.mirror),false);
 assert.throws(()=>runRelease({root:f.root,checkAssets:()=>{},discover:()=>f.paths,argv:['minor'],run,log:()=>{}}),/已恢复/);
 ['package.json','Cargo.toml','Cargo.lock'].forEach((p,i)=>assert.equal(readFileSync(join(f.root,p),'utf8'),before[i]));
 }finally{f.dispose();}
});
test('自测产物独立、manifest 校验字节和协议；不改版本',()=>{
 const f=fixture();try{
 mkdirSync(join(f.root,'dist'));writeFileSync(join(f.root,'dist/index.html'),'existing production');
 const run=(cmd,args,opts)=>{if(cmd==='git')return args[0]==='status'?'M dirty':'abcdef012345';assert.equal(cmd,'pnpm');if(args[0]==='licenses:generate')return;assert.deepEqual(args,['build','--outDir','dist/test-build']);assert.equal(opts.env.RAYBEND_CHANNEL,'test');assert.equal(opts.env.RAYBEND_PHOTO_AI,'0');mkdirSync(join(f.root,'dist/test-build'));writeFileSync(join(f.root,'dist/test-build/index.html'),'test');};
 runRelease({root:f.root,checkAssets:()=>{},discover:()=>f.paths,argv:['test'],run,log:()=>{}});
 assert.equal(readFileSync(join(f.root,'dist/index.html'),'utf8'),'existing production');
 const manifest=JSON.parse(readFileSync(join(f.root,'dist/test-build/raybend-build.json'),'utf8'));
 assert.equal(manifest.workerProtocol,'fixture-v1');assert.equal(manifest.dirty,true);assert.equal(manifest.files[0].bytes,4);assert.match(manifest.files[0].sha256,/^[a-f0-9]{64}$/);assert.equal(existsSync(join(f.root,'.release')),false);
 assert.equal(JSON.parse(readFileSync(join(f.root,'package.json'),'utf8')).version,'0.1.0');
 }finally{f.dispose();}
});

for(const targets of [['msi'],['nsis'],['nsis','msi']])test(`Windows ${targets.join('/')} 构建一次、镜像和自动收口；自测输出独立`,()=>{
 const f=fixture();try{
 let finalized;
 const calls=[];
 mkdirSync(join(f.target,'release/bundle'),{recursive:true});writeFileSync(join(f.target,'release/bundle/old.msi'),'old');
 const run=(cmd,args,opts)=>{
  calls.push([cmd,...args]);
  if(cmd==='git')return args[0]==='status'?'':'abcdef012345';
  if(cmd==='wslpath')throw new Error('unexpected wslpath');
  if(cmd==='cmd.exe'){
   if(args.at(-1).endsWith('.cmd')){
    assert.equal(opts.cwd,f.mirror);
    assert.equal(existsSync(join(f.target,'release/bundle/old.msi')),false);
    assert.equal(existsSync(`${f.mirror}.lock`),true);
    assert.equal(readFileSync(join(f.mirror,'src/api/dto-contract.json'),'utf8'),'{}');
    assert.equal(readFileSync(join(f.mirror,'dist/index.html'),'utf8'),'synthetic');
    const config=JSON.parse(readFileSync(join(f.mirror,'.release/tauri.release.json'),'utf8'));
    assert.deepEqual(config.bundle.targets,targets);assert.equal(config.build.frontendDist,'../dist');
    assert.equal(config.build.beforeBuildCommand,null);
    assert.match(opts.env.WSLENV,/CARGO_TARGET_DIR/);assert.equal(opts.env.RAYBEND_CHANNEL,'test');assert.equal(opts.env.RAYBEND_PHOTO_AI,'0');
   }
   return 'tauri-cli 2.11.4';
  }
  assert.equal(cmd,'pnpm');
  if(args[0]==='licenses:generate'||args[0]==='check:win'||args[0]==='ai:prepare')return;
  assert.deepEqual(args,['build','--outDir','dist/test-build']);
  mkdirSync(join(f.root,'dist/test-build'),{recursive:true});writeFileSync(join(f.root,'dist/test-build/index.html'),'synthetic');
 };
 runRelease({root:f.root,checkAssets:()=>{},discover:()=>f.paths,argv:['test',...targets.map(t=>`--win-${t}`),'--unsigned'],run,log:()=>{},finalize:opts=>{finalized=opts;}});
 assert.equal(calls.filter(c=>c[0]==='pnpm'&&c[1]==='build').length,1);
 assert.equal(calls.filter(c=>c[0]==='pnpm'&&c[1]==='ai:prepare').length,0);
 assert.deepEqual(JSON.parse(readFileSync(join(f.target,'release/raybend-ai-build.json'),'utf8')).ai,{enabled:false,reason:'source-missing'});
 assert.deepEqual(finalized.requiredTargets,targets);
 assert.equal(finalized.argv[0],join(f.target,'release/bundle'));
 assert.equal(existsSync(`${f.mirror}.lock`),false);
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
 assert.throws(()=>runRelease({root:f.root,checkAssets:()=>{},discover:()=>f.paths,argv:['patch','--win-msi','--unsigned'],run,log:()=>{}}),/cargo install tauri-cli/);
 assert.equal(readFileSync(join(f.root,'package.json'),'utf8'),before);
 }finally{f.dispose();}
});

test('Windows 构建/核查/收口失败时恢复版本并释放锁，不能收口失败产物',()=>{
 for(const failure of ['bundle','check','finalize']) {
  const f=fixture();try {
   const before=['package.json','Cargo.toml','Cargo.lock'].map(p=>readFileSync(join(f.root,p),'utf8'));
   let finalized=false;
   const run=(cmd,args)=>{
    if(cmd==='git')return args[0]==='status'?'':'abcdef012345';
    if(cmd==='wslpath')throw new Error('unexpected wslpath');
    if(cmd==='cmd.exe'){
     if(args.at(-1).endsWith('.cmd')&&failure==='bundle')throw new Error('bundle failed');
     return 'tauri-cli 2.11.4';
    }
    if(args[0]==='build'){mkdirSync(join(f.root,'dist'));writeFileSync(join(f.root,'dist/index.html'),'new');}
    if(args[0]==='check:win'&&failure==='check')throw new Error('check failed');
   };
   assert.throws(()=>runRelease({root:f.root,checkAssets:()=>{},discover:()=>f.paths,argv:['patch','--win-msi','--unsigned'],run,log:()=>{},finalize:()=>{finalized=true;throw new Error('finalize failed');}}),/已恢复/);
   assert.equal(finalized,failure==='finalize');
   before.forEach((value,i)=>assert.equal(readFileSync(join(f.root,['package.json','Cargo.toml','Cargo.lock'][i]),'utf8'),value));
   assert.equal(existsSync(`${f.mirror}.lock`),false);
  }finally{f.dispose();}
 }
});

test('Windows 锁竞争和已有输出在升版、前端构建前停止',()=>{
 const f=fixture();try{
  const original=readFileSync(join(f.root,'package.json'),'utf8');
  const run=(cmd,args)=>{
   if(cmd==='git')return args[0]==='status'?'':'abcdef012345';
   if(cmd==='cmd.exe')return 'tauri-cli 2.11.4';
   if(cmd==='wslpath')throw new Error('unexpected wslpath');
   throw new Error('should not build');
  };
  writeFileSync(`${f.mirror}.lock`,'other task');
  assert.throws(()=>runRelease({root:f.root,checkAssets:()=>{},discover:()=>f.paths,argv:['patch','--win-msi','--unsigned'],run,log:()=>{}}),/锁已存在/);
  assert.equal(readFileSync(`${f.mirror}.lock`,'utf8'),'other task');
  rmSync(`${f.mirror}.lock`);
  mkdirSync(join(f.root,'release-out/v0.1.1'),{recursive:true});
  assert.throws(()=>runRelease({root:f.root,checkAssets:()=>{},discover:()=>f.paths,argv:['patch','--win-msi','--unsigned'],run,log:()=>{}}),/release-out 已存在/);
  assert.equal(readFileSync(join(f.root,'package.json'),'utf8'),original);
 }finally{f.dispose();}
});

test('自定义构建目录优先级和 Windows 子进程的私钥/dav1d/target 路径统一转换',()=>{
 const f=fixture();try{
  const key=join(f.root,'key.txt');writeFileSync(key,'synthetic private key file');
  let discovered,delivered;
  const run=(cmd,args,opts)=>{
   if(cmd==='git')return args[0]==='status'?'':'abcdef012345';
   if(cmd==='wslpath'){
    if(args[0]==='-u'){assert.equal(args[1],'E:\\key.txt');return key;}
    if(args[1]===key)return 'E:\\key.txt';
    assert.equal(args[1],'/custom-mount/f/deps/dav1d');return 'F:\\deps\\dav1d';
   }
   if(cmd==='cmd.exe'){
    if(args.at(-1).endsWith('.cmd'))delivered=opts.env;
    return 'tauri-cli 2.11.4';
   }
   if(args[0]==='build'){mkdirSync(join(f.root,'dist/test-build'),{recursive:true});writeFileSync(join(f.root,'dist/test-build/index.html'),'frontend');}
  };
  const env={RAYBEND_WIN_BUILD_DIR:'E:\\env-build',RAYBEND_DAV1D_WIN_DIR:'/custom-mount/f/deps/dav1d',TAURI_SIGNING_PRIVATE_KEY:'E:\\key.txt',RAYBEND_UPDATER_PUBLIC_KEY:'synthetic public key',WSLENV:'CARGO_TARGET_DIR/p:TAURI_SIGNING_PRIVATE_KEY/p'};
  const discover=options=>{discovered=options.directory;return f.paths;};
  runRelease({root:f.root,checkAssets:()=>{},argv:['test','--win-msi','--unsigned','--with-updater','--win-dir','/custom-mount/d/build'],env,discover,run,log:()=>{},finalize:()=>{}});
  assert.equal(discovered,'/custom-mount/d/build');
  assert.equal(delivered.TAURI_SIGNING_PRIVATE_KEY,'E:\\key.txt');
  assert.equal(delivered.SYSTEM_DEPS_DAV1D_INCLUDE,'F:\\deps\\dav1d\\include');
  assert.equal(delivered.CARGO_TARGET_DIR,f.paths.windowsTarget);
  assert.ok(!delivered.WSLENV.includes('/p'));
  runRelease({root:f.root,checkAssets:()=>{},argv:['test','--win-msi','--unsigned','--dry-run'],env,discover,run,log:()=>{}});
  assert.equal(discovered,'E:\\env-build');
 }finally{f.dispose();}
});

test('required AI failure precedes version edits and frontend, releases locks; dry-run never materializes',()=>{
 const f=fixture();try{
  const original=readFileSync(join(f.root,'package.json'),'utf8'),seen=[];
  const run=(cmd,args)=>{if(cmd==='git')return args[0]==='status'?'':'abcdef012345';if(cmd==='cmd.exe')return 'tauri-cli 2.11.4';throw new Error('must not build');};
  const aiResolver=opts=>{seen.push(opts);if(opts.mode==='required')throw new Error('AI missing');return {enabled:false,features:['custom-protocol'],resources:{}};};
  assert.throws(()=>runRelease({root:f.root,discover:()=>f.paths,argv:['patch','--win-msi','--unsigned','--ai=required'],run,log:()=>{},aiResolver}),/AI missing/);
  assert.equal(readFileSync(join(f.root,'package.json'),'utf8'),original);assert.equal(existsSync(`${f.mirror}.lock`),false);
  runRelease({root:f.root,discover:()=>f.paths,argv:['patch','--win-msi','--unsigned','--ai=auto','--dry-run'],run,log:()=>{},aiResolver});assert.equal(seen.at(-1).materialize,false);assert.equal(existsSync(f.mirror),false);
 }finally{f.dispose();}
});
