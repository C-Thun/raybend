import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {parseAiArgs,resolveAiBuild,aiBuildEnv,aiBuildMetadata,ensureBuildRuntime,syncAiOutput,tauriAiArgs,readLimitedJson,matchesAiBuild} from './lib/ai-build.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
function option(args,long,short){const i=args.findIndex(a=>a===long||a===short||a.startsWith(long+'='));if(i<0)return undefined;return args[i].includes('=')?args[i].split('=').slice(1).join('='):args[i+1];}
try{
 const {mode,args}=parseAiArgs(process.argv.slice(2));
 if(!['dev','build','bundle'].includes(args[0])||args.some(a=>a==='--help'||a==='-h'))execFileSync('pnpm',['exec','tauri',...args],{cwd:root,stdio:'inherit'});
 else{
  tauriAiArgs(args,{features:[]}); // Reject conflicting capability flags before preparing any resources.
  const triple=option(args,'--target','-t')??process.env.CARGO_BUILD_TARGET;
  const platform=triple?(triple.includes('windows')?'win32':triple.includes('darwin')?'darwin':'linux'):process.platform;
  const plan=resolveAiBuild({root,mode,platform,fetchRuntime:ensureBuildRuntime});
  const metadata=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1'],{cwd:root,encoding:'utf8',stdio:['ignore','pipe','inherit']}));
  const output=join(metadata.target_directory,...(triple?[triple]:[]),args[0]==='dev'||args.includes('--debug')||args.includes('-d')?'debug':'release');
  if(args[0]==='bundle'){
   const record=readLimitedJson(join(output,'raybend-ai-build.json'));
   if(record.schema!==1||!matchesAiBuild(record.ai,plan))throw new Error('已有 exe 的 AI 能力与当前计划不符，请先用相同 --ai 模式重新 build');
  }
  const env=aiBuildEnv(plan);
  if(args[0]==='dev')syncAiOutput(root,plan,output);
  execFileSync('pnpm',['exec','tauri',...tauriAiArgs(args,plan),'--config',env.TAURI_CONFIG],{cwd:root,stdio:'inherit',env});
  if(args[0]!=='dev')syncAiOutput(root,plan,output);
 }
}catch(error){console.error(`✗ ${error.message}`);process.exitCode=1;}
