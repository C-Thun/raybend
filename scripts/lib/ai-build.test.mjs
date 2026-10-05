import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdirSync,writeFileSync,readFileSync,rmSync,existsSync,symlinkSync} from 'node:fs';
import {join} from 'node:path';
import {aiFixture} from './ai-test-fixture.mjs';
import {parseAiArgs,readAiSource,registerAiSource,resolveAiBuild,aiBuildEnv,syncAiOutput,readLimitedJson,tauriAiArgs,existingAiSource,matchesAiBuild} from './ai-build.mjs';
const quiet=()=>{};
test('AI modes are strict; missing/invalid input downgrades auto, required fails, off reads nothing',t=>{
 const f=aiFixture(t);for(const args of [['--ai'],['--ai=no'],['--ai=auto','--ai=off']])assert.throws(()=>parseAiArgs(args));assert.deepEqual(parseAiArgs(['--','test','--ai=required','--win-msi']),{mode:'required',args:['test','--win-msi']});
 const run=extra=>resolveAiBuild({root:f.root,log:quiet,...extra});assert.equal(run().reason,'source-missing');assert.throws(()=>run({mode:'required'}),/要求 AI/);
 writeFileSync(join(f.root,'ai-model-source.local.json'),'{bad');assert.equal(run().reason,'source-unavailable');assert.equal(run({mode:'off',fetchRuntime:()=>assert.fail()}).reason,'explicit-off');assert.equal(existsSync(join(f.root,'.release')),false);
});
test('source registration is atomic; corruption, LFS pointers, identity, bindings, escapes, links and bounds fail',t=>{
 const f=aiFixture(t);assert.deepEqual(existingAiSource(f.registry),f.pointer);registerAiSource(f.root,existingAiSource(f.registry));const original=readFileSync(join(f.root,'ai-model-source.local.json'));
 for(const patch of [{registryId:'0'.repeat(36)},{artifact:'onnx/../bad'},{profile:'profiles/x/../../../bad'},{artifactSha256:'0'.repeat(64)},{profileManifestSha256:'0'.repeat(64)}])assert.throws(()=>registerAiSource(f.root,{...f.pointer,...patch}));
 assert.deepEqual(readFileSync(join(f.root,'ai-model-source.local.json')),original);
 const plan=resolveAiBuild({root:f.root,mode:'required',fetchRuntime:()=>f.prepare(),log:quiet});
 const encoder=join(f.registry,f.artifact,'image_encoder.onnx'),good=readFileSync(encoder);writeFileSync(encoder,'version https://git-lfs.github.com/spec/v1');assert.throws(()=>readAiSource(f.root),/不符/);syncAiOutput(f.root,plan,join(f.root,'frozen-out'));assert.deepEqual(readFileSync(join(f.root,'frozen-out/ai-model/image_encoder.onnx')),good);writeFileSync(encoder,good);
 rmSync(encoder);symlinkSync(join(f.source,'image_encoder.onnx'),encoder);assert.throws(()=>readAiSource(f.root),/链接/);
 const huge=join(f.root,'huge.json');writeFileSync(huge,' '.repeat(65537));assert.throws(()=>readLimitedJson(huge),/大小/);
});
test('one frozen input drives frontend/features/resources, and on→off→on cleans old output',t=>{
 const f=aiFixture(t);f.prepare();registerAiSource(f.root,f.pointer);const run=extra=>resolveAiBuild({root:f.root,log:quiet,...extra});
 const on=run({mode:'required'});assert.equal(on.enabled,true);assert.deepEqual(on.features,['custom-protocol','photo-ai-runtime']);const env=aiBuildEnv(on,{});assert.equal(env.RAYBEND_PHOTO_AI,'1');assert.match(env.TAURI_CONFIG,/ai-model/);assert.doesNotMatch(JSON.stringify(on.resources),/registry/);
 const output=join(f.root,'out');syncAiOutput(f.root,on,output);writeFileSync(join(output,'ai-runtime/unknown.dll'),'stale');syncAiOutput(f.root,on,output);assert.equal(existsSync(join(output,'ai-runtime/unknown.dll')),false);assert.ok(existsSync(join(output,'ai-runtime/onnxruntime.dll')));
 syncAiOutput(f.root,run({mode:'off'}),output);assert.equal(existsSync(join(output,'ai-runtime')),false);assert.equal(existsSync(join(output,'ai-model')),false);assert.equal(JSON.parse(readFileSync(join(output,'raybend-ai-build.json'))).ai.enabled,false);
 writeFileSync(join(output,'ai-runtime.extra'),'unrelated outside resource folder');syncAiOutput(f.root,run(),output);assert.ok(existsSync(join(output,'ai-model/image_encoder.onnx')));assert.equal(run({platform:'linux'}).reason,'platform-unavailable');
 writeFileSync(join(on.snapshot,'ai-runtime/unknown.dll'),'unexpected');assert.equal(run().enabled,false);rmSync(join(on.snapshot,'ai-runtime/unknown.dll'));writeFileSync(join(on.snapshot,'ai-model/image_encoder.onnx'),'corrupt');assert.equal(run().enabled,false);
});
test('runtime is only prepared for valid model and requested materialization',t=>{
 const f=aiFixture(t);registerAiSource(f.root,f.pointer);let fetched=0;const fetchRuntime=()=>{fetched++;f.prepare();};const opts={root:f.root,log:quiet,fetchRuntime};
 assert.equal(resolveAiBuild({...opts,materialize:false}).enabled,false);assert.equal(fetched,0);assert.equal(resolveAiBuild(opts).enabled,true);assert.equal(fetched,1);
 rmSync(join(f.root,'ai-model-source.local.json'));resolveAiBuild(opts);assert.equal(fetched,1);
});

test('Tauri wrapper merges unrelated features but refuses bypassing native AI capability',()=>{
 const off={features:['custom-protocol']};assert.deepEqual(tauriAiArgs(['build','--features','other,more','--debug'],off),['build','--debug','--features','custom-protocol,other,more']);
 assert.deepEqual(tauriAiArgs(['dev'],off),['dev']);
 for(const args of [['build','--all-features'],['build','-f','photo-ai-runtime'],['build','--features=raybend/ai-runtime'],['build','-f']])assert.throws(()=>tauriAiArgs(args,off));
});

test('separate bundle checks frozen capability and identity; off reasons are diagnostic only',()=>{
 assert.equal(matchesAiBuild({enabled:false,reason:'explicit-off'},{enabled:false,reason:'source-missing'}),true);
 const on={enabled:true,manifestSha256:'a',artifactSha256:'b',runtimeVersion:'1'};
 assert.equal(matchesAiBuild({...on},on),true);for(const patch of [{enabled:false},{manifestSha256:'x'},{artifactSha256:'x'},{runtimeVersion:'2'}])assert.equal(matchesAiBuild({...on,...patch},on),false);assert.equal(matchesAiBuild({},on),false);
});
