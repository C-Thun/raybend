import {test} from "node:test";import assert from "node:assert/strict";import {diagnosticSummary} from "./support-info.ts";
test("诊断摘要只保留构建字段，忽略扩展中的路径、照片和完整 UA",()=>{
 const build={version:"1.2.3",channel:"release" as const,builtAt:"",photoPath:"C:\\私人照片\\婚礼.jpg",library:"secret"};
 const summary=diagnosticSummary(build,"Custom private token Edg/135.2.3");assert.equal(summary.includes("私人"),false);assert.equal(summary.includes("secret"),false);assert.equal(summary.includes("private token"),false);assert.equal(JSON.parse(summary).webview,"135.2.3");assert.equal(JSON.parse(diagnosticSummary(build,"" )).webview,"unknown");
});
