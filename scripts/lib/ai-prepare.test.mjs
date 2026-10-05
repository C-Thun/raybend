import {test} from 'node:test';import assert from 'node:assert/strict';
import {writeFileSync,readFileSync,rmSync,readdirSync} from 'node:fs';import {join} from 'node:path';import {deflateRawSync} from 'node:zlib';
import {aiFixture,sha} from './ai-test-fixture.mjs';import {runtimeZipFiles,ensureCpuRuntime} from './ai-prepare.mjs';
function zip(files,source){const bodies=[],entries=[];let offset=0;for(const f of files){const name=Buffer.from(`onnxruntime-win-x64-1.28.0/lib/${f.name}`),data=readFileSync(join(source,f.name)),body=deflateRawSync(data),local=Buffer.alloc(30),central=Buffer.alloc(46);local.writeUInt32LE(0x04034b50);local.writeUInt16LE(8,8);local.writeUInt32LE(body.length,18);local.writeUInt32LE(data.length,22);local.writeUInt16LE(name.length,26);central.writeUInt32LE(0x02014b50);central.writeUInt16LE(8,10);central.writeUInt32LE(body.length,20);central.writeUInt32LE(data.length,24);central.writeUInt16LE(name.length,28);central.writeUInt32LE(offset,42);bodies.push(local,name,body);entries.push(central,name);offset+=local.length+name.length+body.length;}const index=Buffer.concat(entries),end=Buffer.alloc(22);end.writeUInt32LE(0x06054b50);end.writeUInt16LE(files.length,8);end.writeUInt16LE(files.length,10);end.writeUInt32LE(index.length,12);end.writeUInt32LE(offset,16);return Buffer.concat([...bodies,index,end]);}
function archive(f){const b=zip(f.files.slice(3),f.source),path=join(f.runtime,'runtime.json'),info=JSON.parse(readFileSync(path));info.archive={bytes:b.length,sha256:sha(b),url:'https://example.invalid/cpu.zip'};writeFileSync(path,JSON.stringify(info));return b;}
test('bounded ZIP extracts only pinned entries and refuses truncation, duplicates and wrong digests',t=>{
 const f=aiFixture(t),b=archive(f),files=f.files.slice(3);assert.equal(runtimeZipFiles(b,files).size,2);
 for(const value of [Buffer.alloc(0),b.subarray(0,20),b.subarray(0,b.length-1)])assert.throws(()=>runtimeZipFiles(value,files));assert.throws(()=>runtimeZipFiles(b,[{...files[0],sha256:'0'.repeat(64)},files[1]]));assert.throws(()=>runtimeZipFiles(zip([files[0],files[0]],f.source),files));
});
test('verified HTTPS cache works offline, corrupt download is retried and never installed, redirects stay HTTPS',async t=>{
 const f=aiFixture(t),b=archive(f);let calls=0;const opts={root:f.root,env:{},log:()=>{},fetcher:async()=>{calls++;return new Response(b,{status:200,headers:{'content-length':String(b.length)}});}};
 await ensureCpuRuntime(opts);assert.equal(calls,1);for(const file of f.files.slice(3))rmSync(join(f.runtime,file.name));await ensureCpuRuntime({...opts,fetcher:()=>assert.fail('offline cache should suffice')});
 for(const file of f.files.slice(3))rmSync(join(f.runtime,file.name));const cache=join(f.root,'.cache/ai-build/v1');for(const n of readdirSync(cache))rmSync(join(cache,n));calls=0;
 await assert.rejects(ensureCpuRuntime({...opts,fetcher:async()=>{calls++;return new Response(Buffer.alloc(b.length),{status:200});}}),/获取失败/);assert.equal(calls,2);assert.equal(readdirSync(cache).length,0);
 await assert.rejects(ensureCpuRuntime({...opts,fetcher:async()=>new Response(null,{status:302,headers:{location:'http://example.invalid/unsafe'}})}),/获取失败/);assert.equal(readdirSync(cache).length,0);
});
test('local fixed runtime skips network and lock race is rejected without stealing lock',async t=>{
 const f=aiFixture(t);archive(f);await ensureCpuRuntime({root:f.root,env:{},runtimeDirectory:f.source,fetcher:()=>assert.fail()});
 for(const file of f.files.slice(3))rmSync(join(f.runtime,file.name));let release;const blocked=new Promise(r=>release=r);const first=ensureCpuRuntime({root:f.root,env:{},log:()=>{},fetcher:async()=>{await blocked;return new Response(null,{status:404});}});
 await assert.rejects(ensureCpuRuntime({root:f.root,env:{},fetcher:()=>assert.fail()}),/锁已存在/);release();await assert.rejects(first,/获取失败/);
});
