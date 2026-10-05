import {dirname,resolve} from 'node:path';import {fileURLToPath} from 'node:url';import {ensureCpuRuntime} from '../lib/ai-prepare.mjs';
try{await ensureCpuRuntime({root:resolve(dirname(fileURLToPath(import.meta.url)),'../..')});}catch(e){console.error(`✗ ${e.message}`);process.exitCode=1;}
