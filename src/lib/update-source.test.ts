import { test } from "node:test";import assert from "node:assert/strict";
import {validUpdateSource,readUpdateSource,defaultUpdateSource,readAutoCheck,autoCheckSource} from "./update-source.ts";
test("未配置默认关闭；自定义源和自定义公钥绑定，关闭偏好优先",()=>{
 const fallback=defaultUpdateSource("official","release");assert.equal(defaultUpdateSource("","release").mode,"off");assert.equal(defaultUpdateSource("pub","beta").mode,"beta");
 const custom={mode:"custom" as const,endpoint:"https://fork.example/update",publicKey:"fork"};assert.deepEqual(readUpdateSource(JSON.stringify(custom),fallback),custom);assert.equal(readUpdateSource('{"mode":"off"}',fallback).mode,"off");assert.deepEqual(readUpdateSource('garbage',fallback),fallback);
});
test("拒绝无公钥、HTTP、本地文件、凭据、片段及超长配置",()=>{
 for(const endpoint of ["http://example.org","file:///a","https://me:pass@example.org/u","https://example.org/#secret",""])assert.equal(validUpdateSource({mode:"custom",endpoint,publicKey:"k"}),false);
 assert.equal(validUpdateSource({mode:"custom",endpoint:"https://example.org",publicKey:""}),false);
 assert.equal(validUpdateSource({mode:"custom",endpoint:"https://example.org",publicKey:"k".repeat(8193)}),false);
 assert.equal(validUpdateSource({mode:"stable",endpoint:"https://example.org",publicKey:"k"}),true);
});

test("开机静默检查：默认开，只有显式 off 才关",()=>{
 assert.equal(readAutoCheck(null),true,"没存过 = 默认开");
 assert.equal(readAutoCheck("on"),true);
 assert.equal(readAutoCheck("off"),false);
 assert.equal(readAutoCheck("garbage"),true,"损坏偏好不静默地把用户的路堵上");
});

test("开机静默检查的源：用户偏好优先；off / 无公钥不查",()=>{
 const fallback=defaultUpdateSource("official","release");
 assert.equal(autoCheckSource(null,defaultUpdateSource("","release")),null,"构建不带公钥时没有可验证的源");
 assert.deepEqual(autoCheckSource(null,fallback),fallback);
 assert.equal(autoCheckSource(JSON.stringify({mode:"off"}),fallback),null,"用户关过就尊重");
 assert.deepEqual(autoCheckSource(JSON.stringify({mode:"stable",endpoint:"",publicKey:""}),fallback),fallback,"残缺偏好回到构建默认（只信可验证的源）");
 const custom={mode:"custom" as const,endpoint:"https://fork.example/update",publicKey:"fork"};
 assert.deepEqual(autoCheckSource(JSON.stringify(custom),fallback),custom,"自定义源照用");
 assert.deepEqual(autoCheckSource("garbage",fallback),fallback,"损坏偏好回到构建默认");
});
