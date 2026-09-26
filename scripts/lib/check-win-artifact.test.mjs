import {test} from 'node:test';import assert from 'node:assert/strict';import {execFileSync} from 'node:child_process';
import {mkdtempSync,mkdirSync,writeFileSync,utimesSync,rmSync} from 'node:fs';import {tmpdir} from 'node:os';import {join,resolve} from 'node:path';
const checker=resolve('scripts/check-win-artifact.mjs');
test('自重启模式不依赖第二份 worker，但协议或前端缺失仍拒绝；默认模式仍要求独立 worker',()=>{
 const root=mkdtempSync(join(tmpdir(),'raybend-win-artifact-'));try{
 mkdirSync(join(root,'dist/splash'),{recursive:true});mkdirSync(join(root,'crates/raybend/src/raw'),{recursive:true});
 writeFileSync(join(root,'dist/index.html'),'<script src="assets/main.js"></script>');writeFileSync(join(root,'dist/splash.html'),'splash/splash-');writeFileSync(join(root,'dist/splash/splash-cn.webp'),'image fixture');
 writeFileSync(join(root,'crates/raybend/src/raw/worker.rs'),'pub const PROTOCOL_TAG: &str = "proto-current";');
 const exe=join(root,'app.exe');const write=text=>{writeFileSync(exe,text);const future=new Date(Date.now()+5000);utimesSync(exe,future,future);};
 const run=mode=>execFileSync(process.execPath,[checker],{cwd:root,encoding:'utf8',stdio:['ignore','pipe','pipe'],env:{...process.env,WIN_DIST:'dist',WIN_EXE:exe,WIN_WORKER_MODE:mode}});
 write('assets/main.js splash/splash-cn.webp proto-current');assert.match(run('self'),/主程序自重启/);assert.throws(()=>run('standalone'));
 write('assets/main.js splash/splash-cn.webp proto-old');assert.throws(()=>run('self'));
 write('assets/old.js splash/splash-cn.webp proto-current');assert.throws(()=>run('self'));
 }finally{rmSync(root,{recursive:true,force:true});}
});
