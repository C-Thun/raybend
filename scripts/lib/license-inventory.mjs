import { readFileSync, readdirSync, existsSync, statSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
export function windowsDependencies(metadata) {
 const nodes=new Map(metadata.resolve.nodes.map(n=>[n.id,n]));const selected=new Set(metadata.workspace_members),queue=[...selected];
 while(queue.length){for(const d of nodes.get(queue.pop())?.deps??[]){if(d.dep_kinds.some(k=>k.kind!=='dev')&&!selected.has(d.pkg)){selected.add(d.pkg);queue.push(d.pkg);}}}
 return metadata.packages.filter(p=>selected.has(p.id)&&p.source);
}
export function licenseTexts(directory,licenseFile){
 const paths=new Set();const scan=(dir,depth)=>{for(const file of readdirSync(dir,{withFileTypes:true})){const path=join(dir,file.name);if(file.isFile() && /^(licen[cs]e|copying|copyright|notice|patents)([._-]|$)/i.test(file.name))paths.add(path);else if(file.isDirectory() && depth<2 && /^(licenses?|legal|vendor)$/i.test(file.name))scan(path,depth+1);}};scan(directory,0);
 if(licenseFile){const path=join(directory,licenseFile);if(existsSync(path)&&statSync(path).isFile())paths.add(path);}
 return [...paths].sort().map(path=>({file:relative(directory,path).replaceAll('\\','/'),text:readFileSync(path,'utf8')}));
}
export function cargoLicenses(metadata,overrides,standardTexts) {
 return windowsDependencies(metadata).map(p=>{
  const texts=licenseTexts(dirname(p.manifest_path),p.license_file),key=`${p.name}@${p.version}`;
  if(overrides[key])texts.push({file:'upstream-license',...overrides[key]});
  let provenance=overrides[key]?.provenance??'package';
  if(!texts.some(t=>/(?:^|\/)(?:licen[cs]e|copying|upstream)/i.test(t.file))){
   const id=p.license?.split(/ OR |\//)[0];const text=standardTexts[id];
   if(!text)throw new Error(`许可证原文缺失：${key} (${p.license??'unknown'})`);
   texts.push({file:`SPDX-${id}`,source:`https://github.com/spdx/license-list-data/tree/v3.28.0/text/${id}.txt`,text});provenance='spdx-fallback';
  }
  return {ecosystem:'cargo',name:p.name,version:p.version,license:p.license??'LicenseRef-package-file',source:p.repository??`https://crates.io/crates/${p.name}/${p.version}`,authors:p.authors??[],provenance,texts};
 });
}
export function pnpmLicenses(report){
 return Object.values(report).flat().flatMap(p=>p.versions.map((version,i)=>{
  const directory=p.paths[i]??p.paths[0],texts=licenseTexts(directory);
  if(!texts.length)throw new Error(`pnpm 许可证原文缺失：${p.name}@${version}`);
  return {ecosystem:'pnpm',name:p.name,version,license:p.license,source:p.homepage??`https://www.npmjs.com/package/${p.name}/v/${version}`,authors:p.author?[p.author]:[],provenance:'package',texts};
 }));
}
