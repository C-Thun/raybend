import {dirname,resolve} from 'node:path';import {fileURLToPath} from 'node:url';
import {parseAiArgs,resolveAiBuild,ensureBuildRuntime} from '../lib/ai-build.mjs';
try{const {mode,args}=parseAiArgs(process.argv.slice(2));if(args.length)throw new Error('用法：pnpm ai:prepare [--ai=auto|required|off]');const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');console.log(JSON.stringify(resolveAiBuild({root,mode,fetchRuntime:ensureBuildRuntime,log:console.error})));}catch(e){console.error(`✗ ${e.message}`);process.exitCode=1;}
