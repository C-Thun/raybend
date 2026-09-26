import {test} from 'node:test';import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync} from 'node:fs';import {tmpdir} from 'node:os';import {join} from 'node:path';
import {windowsDependencies,cargoLicenses} from './license-inventory.mjs';
test('只沿 normal/build 依赖收录；循环不重复，dev 不进入图',()=>{
 const m={workspace_members:['root'],resolve:{nodes:[{id:'root',deps:[{pkg:'a',dep_kinds:[{kind:null}]},{pkg:'b',dep_kinds:[{kind:'dev'}]}]},{id:'a',deps:[{pkg:'root',dep_kinds:[{kind:'build'}]}]}]},packages:[{id:'root',source:null},{id:'a',source:'registry'},{id:'b',source:'registry'}]};assert.deepEqual(windowsDependencies(m).map(p=>p.id),['a']);
});
test('缺失许可阻断；上游补足保留来源，不泄漏本机路径',()=>{
 const root=mkdtempSync(join(tmpdir(),'raybend-license-'));try{
 const p={id:'a',source:'registry',name:'中文',version:'1',manifest_path:join(root,'Cargo.toml'),license:'MIT',authors:['Author']};
 const m={workspace_members:['a'],resolve:{nodes:[{id:'a',deps:[]}]},packages:[p]};
 assert.throws(()=>cargoLicenses(m,{},{}),/原文缺失/);
 const rows=cargoLicenses(m,{'中文@1':{source:'https://example.org/LICENSE',text:'original copyright and terms'}},{});assert.equal(rows[0].texts[0].source,'https://example.org/LICENSE');assert.equal(JSON.stringify(rows).includes(root),false);
 mkdirSync(join(root,'vendor'));writeFileSync(join(root,'vendor/COPYING'),'native license');assert.equal(cargoLicenses(m,{},{} )[0].texts[0].file,'vendor/COPYING');
 }finally{rmSync(root,{recursive:true,force:true});}
});
