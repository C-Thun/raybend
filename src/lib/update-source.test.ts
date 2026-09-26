import { test } from "node:test";import assert from "node:assert/strict";
import {validUpdateSource,readUpdateSource,defaultUpdateSource} from "./update-source.ts";
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
