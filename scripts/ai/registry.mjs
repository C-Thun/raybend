/** RayBend adapter: generic exporter produces bytes; this project selects a trusted profile. */
import {execFileSync} from 'node:child_process';
import {existsSync,readFileSync} from 'node:fs';
import {dirname,join,resolve} from 'node:path';
import {homedir} from 'node:os';
import {fileURLToPath} from 'node:url';
import {registerAiSource,existingAiSource} from '../lib/ai-build.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const action=process.argv[2],args=process.argv.slice(3).filter(a=>a!=='--');
try{
 if(!['init','export','use'].includes(action))throw new Error('用法：init|export|use [registry路径] [--import 导出目录] [--python Python路径]');
 const data=process.platform==='win32'?process.env.LOCALAPPDATA:process.env.XDG_DATA_HOME?.startsWith('/')?process.env.XDG_DATA_HOME:join(homedir(),'.local/share');
 if(!data)throw new Error('无法确定用户数据目录，请设置 LOCALAPPDATA 或提供有效的系统用户目录');
 let registry=args[0]&&!args[0].startsWith('--')?resolve(args.shift()):join(data,'model-registry/library-v1');
 const toolkit=existsSync(join(registry,'scripts/cli.mjs'))?registry:resolve(root,'../model-registry');
 if(action!=='use'&&!existsSync(join(toolkit,'scripts/cli.mjs')))throw new Error('未找到 model-registry 导出工具，请先签出该仓库或提供路径');
 if(action==='init'||!existsSync(join(registry,'.model-registry.json'))){if(action==='use'||action!=='init'&&registry!==join(data,'model-registry/library-v1'))throw new Error('指定目录尚未初始化，请先执行 pnpm ai:library:init');execFileSync(process.execPath,[join(toolkit,'scripts/cli.mjs'),'init',registry],{stdio:'inherit'});}
 if(action==='init'){if(args.length)throw new Error('init 不接受附加参数');console.log('✓ model-registry 已初始化');}
 else{
  let source;
  if(action==='export'){
   for(let i=0;i<args.length;i+=2)if(!['--import','--python'].includes(args[i])||!args[i+1]||args[i+1].startsWith('--'))throw new Error('export 仅支持 --import / --python 路径');
   source=JSON.parse(execFileSync(process.execPath,[join(toolkit,'scripts/cli.mjs'),'export',registry,...args,'--profile-dir',join(root,'crates/raybend/assets/ai/tinyclip-v1'),'--profile-id','raybend-photo-tags','--profile-version','v1'],{encoding:'utf8',stdio:['ignore','pipe','inherit']}));
  }else{
   if(args.length)throw new Error('use 不接受附加参数');source=existingAiSource(registry);
  }
  const {schema,kind,registryRoot,registryId,artifact,artifactSha256,profile,profileManifestSha256}=source;
  registerAiSource(root,{schema,kind,registryRoot,registryId,artifact,artifactSha256,profile,profileManifestSha256});console.log('✓ 本项目模型源已校验并登记（Git ignored）');
 }
}catch(error){console.error(`✗ ${error.message}`);process.exitCode=1;}
