import { parseVersion } from '../../src/lib/release-plan.ts';
export function releaseMetadata({manifest,artifacts,baseUrl,withUpdater=false,unsigned=false}) {
 if(manifest.schema!==1 || !['release','beta','test'].includes(manifest.channel))throw new Error('非法构建 manifest');
 if(!Number.isFinite(Date.parse(manifest.builtAt)))throw new Error('非法构建时间');
 const parsed=parseVersion(manifest.version);if(manifest.channel==='beta' && !parsed.prerelease)throw new Error('预览通道须带预发布版本');if(manifest.channel==='release' && parsed.prerelease)throw new Error('稳定版本不能是预发布');
 const names=new Set();for(const file of artifacts){
  if(!file.name || /[\\/#\u0000-\u001f\u007f]/.test(file.name)||names.has(file.name) || !/\.(exe|msi)$/i.test(file.name) || !/^[a-f0-9]{64}$/.test(file.sha256) || !Number.isSafeInteger(file.bytes) || file.bytes<1)throw new Error('非法/重复的发布文件');names.add(file.name);
  if(!unsigned && file.verified!==true)throw new Error(`发布文件未通过 Authenticode 验证：${file.name}`);
 }
 if(!artifacts.length)throw new Error('没有安装器');
 const checksums=artifacts.map(file=>`${file.sha256}  ${file.name}`).join('\n')+'\n';
 const index={schema:1,version:manifest.version,channel:manifest.channel,builtAt:manifest.builtAt,gitHash:manifest.gitHash,dirty:manifest.dirty,authenticode:unsigned?'unsigned':'verified',artifacts:artifacts.map(({signature,...file})=>file)};
 let update;
 if(withUpdater){
  const base=new URL(baseUrl);if(base.protocol!=='https:'||base.username||base.password||base.hash||base.search)throw new Error('安装包基址必须无凭据 HTTPS URL');if(!base.pathname.endsWith('/'))base.pathname+='/';
  const platforms={};for(const file of artifacts){const type=file.name.toLowerCase().endsWith('.exe')?'nsis':'msi';if(!type)continue;
   if(typeof file.signature!=='string'||!file.signature.trim())throw new Error(`缺少更新签名：${file.name}`);
   const key=`windows-x86_64-${type}`;if(platforms[key])throw new Error('同一更新目标有多个安装器');platforms[key]={signature:file.signature.trim(),url:new URL(encodeURIComponent(file.name),base).href};
  }
  if(!Object.keys(platforms).length)throw new Error('没有 Windows 更新安装器');
  update={version:manifest.version,pub_date:manifest.builtAt,notes:'',platforms};
 }
 return {checksums,index,update};
}
