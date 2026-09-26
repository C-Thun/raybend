#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import { readFileSync,writeFileSync,mkdirSync } from 'node:fs';
import { resolve,dirname } from 'node:path';import { fileURLToPath } from 'node:url';
import { sha256 } from './lib/release-files.mjs';import {cargoLicenses,pnpmLicenses} from './lib/license-inventory.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const run=(cmd,args)=>execFileSync(cmd,args,{cwd:root,encoding:'utf8',maxBuffer:64*1024*1024});
const cargo=JSON.parse(run('cargo',['metadata','--locked','--filter-platform','x86_64-pc-windows-msvc','--format-version','1']));
const pnpm=JSON.parse(run('pnpm',['licenses','list','--prod','--json']));
const read=path=>readFileSync(resolve(root,path),'utf8');const overrides=JSON.parse(read('legal/upstream-license-overrides.json'));
const standardTexts={'MIT':read('legal/MIT.txt'),'MPL-2.0':read('legal/MPL-2.0.txt')};
const components=[...cargoLicenses(cargo,overrides,standardTexts),...pnpmLicenses(pnpm)];
components.push(
 {ecosystem:'native',name:'dav1d',version:'1.5.0',license:'BSD-2-Clause',source:'https://code.videolan.org/videolan/dav1d/-/tree/1.5.0',texts:[{file:'COPYING',text:read('legal/dav1d-1.5.0.txt')}]},
 {ecosystem:'data',name:'LensFun community calibration database',version:'lensfun-0.7.0 bundled snapshot',license:'CC-BY-SA-3.0',source:'https://github.com/lensfun/lensfun',authors:['LensFun community'],texts:[{file:'CC-BY-SA-3.0',text:read('legal/CC-BY-SA-3.0.txt')}]},
 {ecosystem:'source',name:'RayBend source reuse and native library notices',version:'current source',license:'see notices',source:'https://github.com/C-Thun/raybend',texts:[{file:'THIRD-PARTY-NOTICES.md',text:read('THIRD-PARTY-NOTICES.md')},{file:'LICENSE',text:read('LICENSE')}]},
);
components.sort((a,b)=>`${a.ecosystem}/${a.name}/${a.version}`.localeCompare(`${b.ecosystem}/${b.name}/${b.version}`));
const payload={schema:1,target:'x86_64-pc-windows-msvc',scope:'Rust normal/build graph (conservative) and pnpm production dependencies; website excluded',locks:{cargo:sha256(read('Cargo.lock')),pnpm:sha256(read('pnpm-lock.yaml'))},components};
mkdirSync(resolve(root,'public/legal'),{recursive:true});writeFileSync(resolve(root,'public/legal/third-party.json'),JSON.stringify(payload)+'\n');
const fallbacks=components.filter(p=>p.provenance==='spdx-fallback').map(p=>`${p.name}@${p.version}`);
console.log(`许可清单：${components.length} 项；SPDX 原文补足：${fallbacks.join(', ')||'无'}（保留包作者；不发明原始版权年份）`);
