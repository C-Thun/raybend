import {test} from "node:test";
import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync,symlinkSync} from "node:fs";
import {join} from "node:path";
import {tmpdir} from "node:os";
import {prepareAiAssets,checkAiAssets} from "./ai-assets.mjs";
import {aiFixture as fixture} from './ai-test-fixture.mjs';
test("fixed CPU/model inputs are verified, repeatable, and repair generated corruption",t=>{
  const f=fixture(t);f.prepare();f.prepare();checkAiAssets(f.root);
  writeFileSync(join(f.model,"image_encoder.onnx"),"broken");assert.throws(()=>checkAiAssets(f.root),/不符/);f.prepare();checkAiAssets(f.root);
});
test("reject incomplete/corrupt/traversal/symlink inputs before replacing good resources",t=>{
  const f=fixture(t);f.prepare();const before=readFileSync(join(f.model,"image_encoder.onnx"));
  writeFileSync(join(f.source,"onnxruntime.dll"),"broken");assert.throws(f.prepare,/不符/);assert.deepEqual(readFileSync(join(f.model,"image_encoder.onnx")),before);
  rmSync(join(f.source,"onnxruntime.dll"));assert.throws(f.prepare,/缺少/);
  symlinkSync(join(f.runtime,"onnxruntime.dll"),join(f.source,"onnxruntime.dll"));assert.throws(f.prepare,/不符/);
  const path=join(f.model,"manifest.json"),data=JSON.parse(readFileSync(path));data.files[0].name="../outside";writeFileSync(path,JSON.stringify(data));assert.throws(f.prepare,/清单/);
});
