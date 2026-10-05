import {createHash,randomUUID} from 'node:crypto';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {prepareAiAssets} from './ai-assets.mjs';
export const sha=b=>createHash('sha256').update(b).digest('hex');
export function aiFixture(t){
 const root=mkdtempSync(join(tmpdir(),'raybend-ai-中文 空格-'));t.after(()=>rmSync(root,{recursive:true,force:true}));
 const model=join(root,'crates/raybend/assets/ai/tinyclip-v1'),runtime=join(root,'crates/raybend/assets/ai/ort-win-x64'),source=join(root,'incoming');
 for(const d of [model,runtime,source])mkdirSync(d,{recursive:true});
 const files=['image_encoder.onnx','classes.json','LICENSE.txt','onnxruntime.dll','onnxruntime_providers_shared.dll'].map(name=>{
  const b=Buffer.from(`fixture:${name}`);writeFileSync(join(source,name),b);if(['classes.json','LICENSE.txt'].includes(name))writeFileSync(join(model,name),b);return {name,bytes:b.length,sha256:sha(b)};
 });
 const manifest={format:2,thresholds_calibrated:true,ort_version:'1.28.0',opset:17,image_input:'pixel_values',image_output:'image_features',files:files.slice(0,3)};
 writeFileSync(join(model,'manifest.json'),JSON.stringify(manifest));writeFileSync(join(runtime,'runtime.json'),JSON.stringify({version:'1.28.0',platform:'win-x64',files:files.slice(3)}));
 for(const n of ['LICENSE.txt','ThirdPartyNotices.txt'])writeFileSync(join(runtime,n),'upstream license');
 const registry=join(root,'registry'),artifact='onnx/tinyclip/test-r1',profile='profiles/raybend-photo-tags/v1',registryId=randomUUID();
 mkdirSync(join(registry,artifact),{recursive:true});mkdirSync(join(registry,profile),{recursive:true});writeFileSync(join(registry,'.model-registry.json'),JSON.stringify({schema:1,kind:'model-registry',registryId}));
 const meta={schema:1,format:'onnx',id:'tinyclip',version:'test-r1',precision:'fp32',opset:17,contract:{input:{name:'pixel_values',shape:[1,3,224,224]},output:{name:'image_features',shape:[1,512]}},files:[files[0],files[2]]};
 writeFileSync(join(registry,artifact,'artifact.json'),JSON.stringify(meta));
 for(const f of files.slice(0,3)){const b=readFileSync(join(source,f.name));if(f.name==='image_encoder.onnx'||f.name==='LICENSE.txt')writeFileSync(join(registry,artifact,f.name),b);if(f.name!=='image_encoder.onnx')writeFileSync(join(registry,profile,f.name),b);}
 const artifactSha256=sha(JSON.stringify(meta)),profileManifestSha256=sha(JSON.stringify(manifest));writeFileSync(join(registry,profile,'manifest.json'),JSON.stringify(manifest));writeFileSync(join(registry,profile,'profile.json'),JSON.stringify({schema:1,artifact,artifactSha256,manifestSha256:profileManifestSha256}));
 const pointer={schema:1,kind:'local-library',registryRoot:registry,registryId,artifact,artifactSha256,profile,profileManifestSha256};
 return {root,model,runtime,source,files,registry,artifact,profile,pointer,prepare:()=>prepareAiAssets({root,modelDirectory:source,runtimeDirectory:source})};
}
