import {test} from 'node:test';import assert from 'node:assert/strict';import {releaseMetadata} from './release-artifacts.mjs';
const manifest={schema:1,channel:'release',version:'1.2.3',builtAt:'2026-09-26T12:00:00Z',gitHash:'abcdef',dirty:false};
const file={name:'RayBend 中文.exe',sha256:'a'.repeat(64),bytes:12,verified:true,signature:'minisign-data'};
test('校验和与两类更新目标分开，Unicode 下载地址编码',()=>{
 const r=releaseMetadata({manifest,artifacts:[file,{...file,name:'RayBend.msi'}],baseUrl:'https://example.org/v1.2.3',withUpdater:true});assert.match(r.checksums,/中文/);assert.equal(r.update.platforms['windows-x86_64-nsis'].url,'https://example.org/v1.2.3/RayBend%20%E4%B8%AD%E6%96%87.exe');assert.ok(r.update.platforms['windows-x86_64-msi']);assert.equal(r.update.platforms['windows-x86_64'],undefined);
});
test('缺签名、重复、非法路径、HTTP 基址、无 Authenticode 不冒充可发布',()=>{
 for(const changes of [{signature:undefined},{name:'../secret.exe'},{sha256:'bad'},{bytes:0},{verified:false}])assert.throws(()=>releaseMetadata({manifest,artifacts:[{...file,...changes}],baseUrl:'https://example.org/',withUpdater:true}));
 assert.throws(()=>releaseMetadata({manifest,artifacts:[file,file]}));assert.throws(()=>releaseMetadata({manifest,artifacts:[file],baseUrl:'http://example.org',withUpdater:true}));
 const unsigned=releaseMetadata({manifest,artifacts:[{...file,verified:false}],unsigned:true});assert.equal(unsigned.index.authenticode,'unsigned');assert.equal(unsigned.update,undefined);
});

test('非法构建时间、预览标签、非安装器与 gh 标签分隔符拒绝',()=>{
 for(const changes of [{builtAt:'bad'},{channel:'beta',version:'1.2.3'}])assert.throws(()=>releaseMetadata({manifest:{...manifest,...changes},artifacts:[file]}));
 for(const name of ['notes.zip','RayBend#label.exe','RayBend\t.exe','secret\0.exe'])assert.throws(()=>releaseMetadata({manifest,artifacts:[{...file,name}]}));
});
