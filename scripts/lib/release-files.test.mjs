import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { applyVersionEdits, versionEdits, workspaceVersion } from "./release-files.mjs";
import { windowsReleaseCommands, windowsReleaseConfig, cmdPath } from "./release-windows.mjs";
test("版本事务同步三文件、保留其他字段并可恢复",()=>{
  const root=mkdtempSync(join(tmpdir(),"raybend-release-"));
  try {
    writeFileSync(join(root,"package.json"),JSON.stringify({version:"0.1.0",packageManager:"pnpm@12.3.4"},null,2)+"\n");
    writeFileSync(join(root,"Cargo.toml"),'[workspace]\nmembers = []\n\n[workspace.package]\nversion = "0.1.0"\nedition = "2024"\n\n[profile.dev]\nopt-level = 2\n');
    writeFileSync(join(root,"Cargo.lock"),'version = 4\n\n[[package]]\nname = "raybend"\nversion = "0.1.0"\n\n[[package]]\nname = "raybend-desktop"\nversion = "0.1.0"\n');
    const edits=versionEdits(root,"0.2.0-beta.2"), restore=applyVersionEdits(edits);
    assert.equal(workspaceVersion(readFileSync(join(root,"Cargo.toml"),"utf8")),"0.2.0-beta.2");
    assert.equal(JSON.parse(readFileSync(join(root,"package.json"))).packageManager,"pnpm@12.3.4");
    restore();for(const e of edits) assert.equal(readFileSync(e.path,"utf8"),e.before);
    assert.equal(versionEdits(root,"0.1.0").length,0);
    writeFileSync(join(root,"Cargo.toml"),'[workspace.package]\nversion = "9.0.0"\n');
    assert.throws(()=>versionEdits(root,"0.2.0"),/不一致/);
  } finally {rmSync(root,{recursive:true,force:true});}
});
test("失败恢复与外部改动保护",()=>{
  const data={a:"old",b:"old"};let fail=true;
  const io={read:p=>data[p],write:(p,v)=>{if(p==="b"&&fail){fail=false;throw Error("disk full");}data[p]=v;}};
  const edits=[{path:"a",before:"old",after:"new"},{path:"b",before:"old",after:"new"}];
  assert.throws(()=>applyVersionEdits(edits,io),/disk full/);assert.deepEqual(data,{a:"old",b:"old"});
  const rollback=applyVersionEdits(edits,io);data.a="someone else";
  assert.throws(rollback,/外部修改/);assert.deepEqual(data,{a:"someone else",b:"old"});
  assert.throws(()=>applyVersionEdits(edits,io),/已被修改/);
});
test("Windows 配置签名、通道和前端只构建一次",()=>{
  const base={targetVersion:"1.2.3",channel:"release",windowsTargets:["msi"]},env={frontendDist:"../dist",certThumbprint:"A".repeat(40)};
  const stable=windowsReleaseConfig(base,env);assert.deepEqual(stable.bundle.targets,["msi"]);assert.equal(stable.build.beforeBuildCommand,null);
  assert.deepEqual(windowsReleaseConfig({...base,windowsTargets:["nsis","msi"]},env).bundle.targets,["nsis","msi"]);
  assert.equal(stable.bundle.windows.signCommand.args.at(-1),"%1");assert.equal(stable.bundle.windows.allowDowngrades,false);
  assert.equal(windowsReleaseConfig(base,{...env,unsigned:true,certThumbprint:""}).bundle.windows.signCommand,null);
  assert.throws(()=>windowsReleaseConfig(base,{...env,withUpdater:true}),/私钥/);
  assert.throws(()=>windowsReleaseConfig(base,{...env,timestamp:"http://example.com"}),/HTTPS/);
  assert.throws(()=>windowsReleaseConfig(base,{...env,timestamp:"not a url"}),/HTTPS/);
  const beta=windowsReleaseConfig({...base,targetVersion:"1.2.3-beta.1",channel:"beta",windowsTargets:["nsis"]},{frontendDist:"../dist",unsigned:true});
  assert.deepEqual(beta.bundle.targets,["nsis"]);assert.equal(beta.bundle.windows.signCommand,null);
  assert.throws(()=>windowsReleaseConfig({...base,targetVersion:"256.0.0"},env),/MSI/);
  const cmd=windowsReleaseCommands('C:\\中文 相册','C:\\staging area\\config.json');
  assert.ok(cmd.includes('cd /d "C:\\中文 相册"'));assert.ok(cmd.includes("--locked"));
  assert.ok(cmd.includes("cargo build -p raybend"));assert.ok(cmd.includes("--features custom-protocol"));
  assert.ok(cmd.includes("cargo tauri bundle -vv"));assert.ok(!cmd.includes("pushd"));
  assert.throws(()=>windowsReleaseCommands('\\\\wsl.localhost\\Ubuntu\\home\\repo','C:\\config.json'),/本地盘/);
  assert.throws(()=>windowsReleaseCommands('C:\\repo','config.json'),/本地盘/);
  for(const path of ['a&echo bad','a%PATH%','a\nb','a"b','a!b'])assert.throws(()=>cmdPath(path));
});
