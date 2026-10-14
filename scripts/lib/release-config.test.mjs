import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {updaterPublicKey,RELEASE_CONFIG_FILE} from './release-config.mjs';

test('更新公钥：环境变量优先，其次仓内公开配置，都没有则为空',t=>{
 const root=mkdtempSync(join(tmpdir(),'raybend-release-config-'));t.after(()=>rmSync(root,{recursive:true,force:true}));
 assert.equal(updaterPublicKey(root,{}),'','缺文件 = 未配置（构建不带更新的包）');
 writeFileSync(join(root,RELEASE_CONFIG_FILE),JSON.stringify({schema:1,updaterPublicKey:'abc'}));
 assert.equal(updaterPublicKey(root,{}),'abc');
 assert.equal(updaterPublicKey(root,{RAYBEND_UPDATER_PUBLIC_KEY:'env-key'}),'env-key','环境变量覆盖仓内配置');
 assert.equal(updaterPublicKey(root,{RAYBEND_UPDATER_PUBLIC_KEY:'  '}),'abc','空白环境变量不算配置');
 writeFileSync(join(root,RELEASE_CONFIG_FILE),JSON.stringify({schema:1,updaterPublicKey:''}));
 assert.equal(updaterPublicKey(root,{}),'');
});

test('更新公钥：坏配置必须显式报错，不能静默发出无更新包',t=>{
 const root=mkdtempSync(join(tmpdir(),'raybend-release-config-'));t.after(()=>rmSync(root,{recursive:true,force:true}));
 for(const bad of [{schema:2,updaterPublicKey:'k'},{schema:1,updaterPublicKey:42},{schema:1,updaterPublicKey:'k'.repeat(8193)},[],'{bad','null']){
  writeFileSync(join(root,RELEASE_CONFIG_FILE),typeof bad==='string'?bad:JSON.stringify(bad));
  assert.throws(()=>updaterPublicKey(root,{}),/release\.config\.json/,`${JSON.stringify(bad)} 应被拒绝`);
 }
});
