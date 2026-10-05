/** One model-registry source and one build capability decision for dev/debug/release. */
import {copyFileSync,existsSync,lstatSync,mkdirSync,readFileSync,realpathSync,renameSync,rmSync,writeFileSync} from 'node:fs';
import {createHash,randomUUID} from 'node:crypto';
import {dirname,join,resolve,sep} from 'node:path';
import {execFileSync} from 'node:child_process';
import {descriptors,verifyAiFile,checkAiAssets} from './ai-assets.mjs';
const hash=b=>createHash('sha256').update(b).digest('hex');
export const AI_OFF_MARKER='raybend-photo-ai-build-v1=off';
export const AI_ON_MARKER='raybend-photo-ai-build-v1=on';
export function parseAiArgs(args){let mode='auto',seen=false;const rest=[];for(let i=0;i<args.length;i++){if(args[i]==='--')continue;if(args[i]==='--ai'||args[i].startsWith('--ai=')){if(seen)throw new Error('重复 --ai');seen=true;mode=args[i]==='--ai'?args[++i]:args[i].slice(5);if(!['auto','required','off'].includes(mode))throw new Error('--ai 必须是 auto|required|off');}else rest.push(args[i]);}return {mode,args:rest};}
export function readLimitedJson(path,max=65536){const stat=lstatSync(path);if(!stat.isFile()||stat.isSymbolicLink()||stat.size>max)throw new Error('AI 元数据类型/大小无效');return JSON.parse(readFileSync(path,'utf8'));}
function inside(root,path){if(typeof path!=='string'||path.includes('\\')||path.split('/').some(p=>!p||p==='.'||p==='..'||!/^[a-zA-Z0-9_.-]+$/.test(p)))throw new Error('AI 引用路径无效');let target=root;for(const p of path.split('/')){target=join(target,p);if(existsSync(target)&&lstatSync(target).isSymbolicLink())throw new Error('AI 引用路径含链接');}if(!target.startsWith(root+sep))throw new Error('AI 引用路径越界');return target;}
export function readAiSource(root,candidate){
 const source=candidate??readLimitedJson(join(root,'ai-model-source.local.json'));
 if(source.schema!==1||source.kind!=='local-library'||typeof source.registryRoot!=='string'||!source.registryRoot.startsWith('/')&& !/^[A-Za-z]:[\\/]/.test(source.registryRoot))throw new Error('AI 本地源配置无效');
 if(lstatSync(source.registryRoot).isSymbolicLink()||!lstatSync(source.registryRoot).isDirectory())throw new Error('AI registry 根无效');const registry=realpathSync(source.registryRoot),marker=readLimitedJson(join(registry,'.model-registry.json'),2048);
 if(marker.schema!==1||marker.kind!=='model-registry'||typeof marker.registryId!=='string'||!/^[-a-f0-9]{36}$/.test(marker.registryId)||marker.registryId!==source.registryId)throw new Error('AI registry 身份不符');
 if(typeof source.artifact!=='string'||source.artifact.split('/').length!==3||source.artifact.split('/')[0]!=='onnx'||typeof source.profile!=='string'||source.profile.split('/').length!==3||source.profile.split('/')[0]!=='profiles')throw new Error('AI artifact/profile 必须是三级定位');
 const directory=inside(registry,source.artifact),profileDirectory=inside(registry,source.profile);readLimitedJson(inside(registry,`${source.artifact}/artifact.json`));const artifactRaw=readFileSync(inside(registry,`${source.artifact}/artifact.json`));if(artifactRaw.length>65536||hash(artifactRaw)!==source.artifactSha256)throw new Error('AI artifact 元数据摘要不符');
 const artifact=JSON.parse(artifactRaw);if(artifact.schema!==1||artifact.format!=='onnx'||artifact.id!==source.artifact.split('/')[1]||artifact.version!==source.artifact.split('/')[2]||!Array.isArray(artifact.files)||artifact.files.length>64||new Set(artifact.files.map(f=>f.name)).size!==artifact.files.length)throw new Error('AI artifact 格式无效');
 for(const f of artifact.files)verifyAiFile(inside(registry,`${source.artifact}/${f.name}`),f);
 const profile=readLimitedJson(join(profileDirectory,'profile.json'));if(profile.schema!==1||profile.artifact!==source.artifact||profile.artifactSha256!==source.artifactSha256||profile.manifestSha256!==source.profileManifestSha256)throw new Error('AI profile 与模型绑定不符');
 const descriptor=descriptors(root);readLimitedJson(join(profileDirectory,'manifest.json'));const manifestRaw=readFileSync(join(profileDirectory,'manifest.json'));if(manifestRaw.length>65536||hash(manifestRaw)!==source.profileManifestSha256||source.profileManifestSha256!==descriptor.manifestSha256)throw new Error('AI 标签包不属于已准入版本');
 const model=descriptor.files.filter(f=>f.kind==='model');for(const f of model){const dir=f.name==='image_encoder.onnx'?directory:profileDirectory;verifyAiFile(join(dir,f.name),f);}
 const image=artifact.files.find(f=>f.name==='image_encoder.onnx'),expected=model.find(f=>f.name==='image_encoder.onnx');if(!image||image.sha256!==expected.sha256||image.bytes!==expected.bytes||artifact.opset!==descriptor.manifest.opset||artifact.precision!=='fp32'||artifact.contract?.input?.name!==descriptor.manifest.image_input||artifact.contract?.output?.name!==descriptor.manifest.image_output||JSON.stringify(artifact.contract.input.shape)!=='[1,3,224,224]'||JSON.stringify(artifact.contract.output.shape)!=='[1,512]')throw new Error('AI 编码器契约不匹配');
 return {source,directory,profileDirectory,descriptor};
}
export function existingAiSource(registryRoot,profile='profiles/raybend-photo-tags/v1'){
 registryRoot=resolve(registryRoot);if(lstatSync(registryRoot).isSymbolicLink())throw new Error('AI registry 根含链接');registryRoot=realpathSync(registryRoot);
 const marker=readLimitedJson(join(registryRoot,'.model-registry.json'),2048),p=readLimitedJson(inside(registryRoot,`${profile}/profile.json`));
 return {schema:1,kind:'local-library',registryRoot,registryId:marker.registryId,artifact:p.artifact,artifactSha256:p.artifactSha256,profile,profileManifestSha256:p.manifestSha256};
}
export function registerAiSource(root,source){
 const path=join(resolve(root),'ai-model-source.local.json'),stage=`${path}.stage-${randomUUID()}`;
 readAiSource(root,source);
 try{writeFileSync(stage,JSON.stringify(source,null,2)+'\n',{flag:'wx'});renameSync(stage,path);}
 finally{rmSync(stage,{force:true});}
}
export function runtimeReady(descriptor){for(const f of descriptor.files.filter(f=>f.kind==='runtime'))verifyAiFile(join(f.directory,f.name),f);for(const name of ['LICENSE.txt','ThirdPartyNotices.txt']){const p=join(descriptor.files.find(f=>f.kind==='runtime').directory,name);if(!lstatSync(p).isFile()||lstatSync(p).isSymbolicLink())throw new Error('CPU 运行库许可缺失');}}
export function resolveAiBuild({root,mode='auto',materialize=true,platform='win32',fetchRuntime,log=console.log}={}){
 root=resolve(root);if(!['auto','required','off'].includes(mode))throw new Error('AI 模式无效');let plan={schema:1,enabled:false,reason:'explicit-off',features:['custom-protocol'],resources:{}};
 if(mode!=='off'){
  try{
   if(!existsSync(join(root,'ai-model-source.local.json'))){plan.reason='source-missing';throw new Error('未配置本地模型源');}
   plan.reason='source-unavailable';const data=readAiSource(root);if(platform!=='win32'){plan.reason='platform-unavailable';throw new Error('当前仅提供 Windows x64 CPU 运行库');}
   plan.reason='runtime-unavailable';try{runtimeReady(data.descriptor);}catch(error){if(materialize&&fetchRuntime){fetchRuntime(root);runtimeReady(data.descriptor);}else throw error;}
   let snapshot=null;
   if(materialize){
    const parent=inside(root,'.release/ai-input-v1'),target=inside(root,`.release/ai-input-v1/${data.descriptor.manifestSha256}`),stage=join(parent,`.stage-${randomUUID()}`);mkdirSync(parent,{recursive:true});
    if(existsSync(target)){checkAiAssets(root,target);}
    else{try{for(const d of ['ai-model','ai-runtime'])mkdirSync(join(stage,d),{recursive:true});for(const f of data.descriptor.files){const dir=f.kind==='runtime'?f.directory:f.name==='image_encoder.onnx'?data.directory:data.profileDirectory;copyFileSync(join(dir,f.name),join(stage,f.kind==='model'?'ai-model':'ai-runtime',f.name));verifyAiFile(join(stage,f.kind==='model'?'ai-model':'ai-runtime',f.name),f);}copyFileSync(join(data.profileDirectory,'manifest.json'),join(stage,'ai-model/manifest.json'));for(const name of ['LICENSE.txt','ThirdPartyNotices.txt','runtime.json'])copyFileSync(join(root,'crates/raybend/assets/ai/ort-win-x64',name),join(stage,'ai-runtime',name));renameSync(stage,target);}finally{rmSync(stage,{recursive:true,force:true});}}
    checkAiAssets(root,target);snapshot=target;
   }
   plan={schema:1,enabled:true,reason:'ready',features:['custom-protocol','photo-ai-runtime'],manifestSha256:data.descriptor.manifestSha256,artifactSha256:data.source.artifactSha256,runtimeVersion:data.descriptor.runtime.version,snapshot,resources:materialize?aiResourceMap(root,snapshot):{}};
  }catch(error){if(mode==='required')throw new Error(`要求 AI，但模型或 CPU 资源不可用：${error.message}`);plan.message=error.message;}
 }
 log(plan.enabled?`✓ AI 已启用：TinyCLIP · CPU ORT ${plan.runtimeVersion}`:`⚠ AI 未启用：${plan.message??'显式关闭'}；继续构建基础版本`);return plan;
}
export function aiResourceMap(root,snapshot){const base=snapshot.slice(root.length+1).replaceAll('\\','/');return {[`../${base}/ai-model/*`]:'ai-model/',[`../${base}/ai-runtime/*`]:'ai-runtime/'};}
export function aiBuildMetadata(plan){return {enabled:plan.enabled,reason:plan.reason,...(plan.enabled?{manifestSha256:plan.manifestSha256,artifactSha256:plan.artifactSha256,runtimeVersion:plan.runtimeVersion}:{})};}
export function aiBuildEnv(plan,env=process.env){return {...env,RAYBEND_PHOTO_AI:plan.enabled?'1':'0',TAURI_CONFIG:JSON.stringify({bundle:{resources:{'../LICENSE':'licenses/LICENSE','../THIRD-PARTY-NOTICES.md':'licenses/THIRD-PARTY-NOTICES.md',...plan.resources}}})};}
export function ensureBuildRuntime(root){execFileSync(process.execPath,[join(root,'scripts/ai/runtime-assets.mjs')],{cwd:root,stdio:'inherit'});}

export function syncAiOutput(root,plan,output){
 if(plan.enabled)checkAiAssets(root,plan.snapshot);
 mkdirSync(output,{recursive:true});
 for(const directory of ['ai-model','ai-runtime']){
  const target=join(output,directory);
  rmSync(target,{recursive:true,force:true});
  if(!plan.enabled)continue;
  mkdirSync(target,{recursive:true});
  const files=directory==='ai-model'?['manifest.json','image_encoder.onnx','classes.json','LICENSE.txt']:['runtime.json','onnxruntime.dll','onnxruntime_providers_shared.dll','LICENSE.txt','ThirdPartyNotices.txt'];
  for(const name of files)copyFileSync(join(plan.snapshot,directory,name),join(target,name));
 }
 if(plan.enabled)checkAiAssets(root,output);
 writeFileSync(join(output,'raybend-ai-build.json'),JSON.stringify({schema:1,ai:aiBuildMetadata(plan)},null,2)+'\n');
}
/** Keep the native inference feature owned by the capability resolver, while preserving unrelated features. */
export function tauriAiArgs(args,plan){
 const out=[],extras=[];
 for(let i=0;i<args.length;i++){
  const a=args[i];if(a==='--all-features')throw new Error('统一构建不接受 --all-features；AI 请用 --ai=required');
  if(a==='--features'||a==='-f'||a.startsWith('--features=')){
   const value=a.startsWith('--features=')?a.slice(11):args[++i];if(!value||value.startsWith('-'))throw new Error('features 缺少值');
   extras.push(...value.split(/[ ,]+/).filter(Boolean));while(args[i+1]&&!args[i+1].startsWith('-'))extras.push(...args[++i].split(/[ ,]+/).filter(Boolean));
  }else out.push(a);
 }
 if(extras.some(f=>['ai-runtime','photo-ai-runtime','ai-probe'].includes(f.split('/').at(-1))))throw new Error('AI feature 由模型源校验决定，请使用 --ai=auto|required|off');
 if(extras.some(f=>!/^[a-zA-Z0-9_/-]+$/.test(f)))throw new Error('feature 名称无效');
 const features=[...new Set([...plan.features.filter(f=>args[0]!=='dev'||f!=='custom-protocol'),...extras])];
 return [...out,...(features.length?['--features',features.join(',')]:[])];
}
/** A bundle must have the same native capability and, when enabled, the same frozen inputs. */
export function matchesAiBuild(record,plan){
 const expected=aiBuildMetadata(plan);
 return typeof record?.enabled==='boolean'&&record.enabled===expected.enabled&&(!expected.enabled||['manifestSha256','artifactSha256','runtimeVersion'].every(key=>record[key]===expected[key]));
}
