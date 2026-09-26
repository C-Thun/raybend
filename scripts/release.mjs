#!/usr/bin/env node
/** 准备发布；--windows 仅供崔总执行。无 tag/push/上传。 */
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync, readdirSync, statSync, existsSync } from "node:fs";
import { dirname, join, resolve, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { parseReleaseArgs, createReleasePlan } from "../src/lib/release-plan.ts";
import { versionEdits, applyVersionEdits, sha256, RELEASE_SOURCE_FILES } from "./lib/release-files.mjs";
import { windowsReleaseConfig, windowsReleaseCommands, cmdPath } from "./lib/release-windows.mjs";
import { finalizeRelease } from "./finalize-release.mjs";
import { windowsBuildEnv } from "./lib/dav1d-win.mjs";
export function runRelease({root=resolve(dirname(fileURLToPath(import.meta.url)),".."),argv=process.argv.slice(2),env=process.env,run=execFileSync,log=console.log,finalize=finalizeRelease}={}) {
  const request=parseReleaseArgs(argv),pkg=JSON.parse(readFileSync(join(root,"package.json"),"utf8"));
  let dirty=true,gitHash,gitAvailable=false;
  try {
    dirty=String(run("git",["status","--porcelain"],{cwd:root,encoding:"utf8"})).trim().length>0;
    gitHash=String(run("git",["rev-parse","--short=12","HEAD"],{cwd:root,encoding:"utf8"})).trim();gitAvailable=true;
  } catch { /* 正式计划阻断；test 记录来源未知。 */ }
  const plan=createReleasePlan({version:pkg.version,request,dirty,gitHash,gitAvailable});
  const edits=versionEdits(root,plan.targetVersion);
  const config=JSON.parse(readFileSync(join(root,"src-tauri/tauri.conf.json"),"utf8"));
  if(config.version!=="../package.json")throw new Error("Tauri 必须从 ../package.json 读取产品版本");
  const builtAt=env.RAYBEND_BUILD_TIME || new Date().toISOString();
  if(!Number.isFinite(Date.parse(builtAt)))throw new Error("非法 RAYBEND_BUILD_TIME");
  const publicKey=request.withUpdater ? env.RAYBEND_UPDATER_PUBLIC_KEY ?? "" : "";
  const buildEnv={...env,RAYBEND_VERSION:plan.targetVersion,RAYBEND_CHANNEL:plan.channel,RAYBEND_BUILD_TIME:builtAt,RAYBEND_GIT_HASH:gitHash??"",RAYBEND_DIRTY:dirty?"1":"0",RAYBEND_UPDATER_PUBLIC_KEY:publicKey,RAYBEND_DISTRIBUTION:"direct"};
  const winConfig=request.windows ? windowsReleaseConfig(plan,{unsigned:request.unsigned,withUpdater:request.withUpdater,frontendDist:`../${plan.outputDir}`,certThumbprint:env.RAYBEND_SIGN_CERT_SHA1,updaterPublicKey:publicKey,updaterPrivateKey:env.TAURI_SIGNING_PRIVATE_KEY,timestamp:env.RAYBEND_SIGN_TIMESTAMP}):undefined;
  const releaseOut=join(root,"release-out",plan.channel==="test"?`test-${builtAt.replaceAll(":","-")}`:`v${plan.targetVersion}`);
  log(`发布计划：${plan.currentVersion} → ${plan.targetVersion} · ${plan.channel}\n前端：${plan.outputDir}\nWindows：${request.windows ? winConfig.bundle.targets.join(" / ") : "未生成；由崔总执行 --windows"}\n版本同步：${edits.map(e=>relative(root,e.path)).join(" / ")||"无需改动"}`);
  for(const warning of plan.warnings)log(`⚠ ${warning}`);
  for(const blocker of plan.blockers)log(`⛔ ${blocker}`);
  if(request.dryRun){log("dry-run：没有改文件、构建、签名或发布");return plan;}
  if(plan.blockers.length)throw new Error(plan.blockers.join("；"));
  if(request.windows && existsSync(releaseOut))throw new Error("本版本 release-out 已存在；保留已有产物，请先核对，不能重复覆盖");
  if(request.windows){
    const cliVersion=JSON.parse(readFileSync(new URL("../node_modules/@tauri-apps/cli/package.json",import.meta.url),"utf8")).version;
    let windowsCli;
    try {windowsCli=String(run("cmd.exe",["/d","/c","cargo tauri --version"],{cwd:root,encoding:"utf8"}));}
    catch {throw new Error(`Windows cargo-tauri 尚未就绪；先由崔总执行一次：cmd.exe /d /c "cargo install tauri-cli --version ${cliVersion} --locked"`);}
    if(/tauri-cli\s+(\S+)/.exec(windowsCli)?.[1]!==cliVersion)throw new Error(`Windows cargo-tauri 须与前端 CLI ${cliVersion} 一致；请先升级，未改版本或构建`);
  }
  const rollback=applyVersionEdits(edits);
  try {
    if(!request.skipBuild){
      run("pnpm",["licenses:generate"],{cwd:root,stdio:"inherit",env:buildEnv});
      run("pnpm",["build","--outDir",plan.outputDir],{cwd:root,stdio:"inherit",env:buildEnv});
      const frontendRoot=join(root,plan.outputDir),files=[];
      const walk=dir=>{for(const entry of readdirSync(dir,{withFileTypes:true})){const path=join(dir,entry.name);if(entry.isSymbolicLink())throw new Error("发布前端不允许符号链接");if(entry.isDirectory())walk(path);else if(entry.name!=="raybend-build.json")files.push({path:relative(frontendRoot,path).replaceAll("\\","/"),sha256:sha256(readFileSync(path)),bytes:statSync(path).size});}};
      walk(frontendRoot);files.sort((a,b)=>a.path.localeCompare(b.path));
      const protocol=/pub const PROTOCOL_TAG: &str = "([^"]+)"/.exec(readFileSync(join(root,"crates/raybend/src/raw/worker.rs"),"utf8"))?.[1];
      if(!protocol)throw new Error("找不到 RAW worker 协议标签");
      const manifest={schema:1,version:plan.targetVersion,channel:plan.channel,builtAt,gitHash:gitHash??null,dirty,distribution:"direct",workerMode:"self",workerProtocol:protocol,files,sourceFiles:Object.fromEntries(RELEASE_SOURCE_FILES.map(path=>[path,sha256(readFileSync(join(root,path)))]))};
      writeFileSync(join(frontendRoot,"raybend-build.json"),JSON.stringify(manifest,null,2)+"\n");
      if(request.windows){
        const stage=join(root,".release",plan.channel,plan.targetVersion);mkdirSync(stage,{recursive:true});
        const cfg=join(stage,"tauri.release.json"),script=join(stage,"build.cmd");
        // 相对 frontendDist 按 src-tauri 配置目录解析，不能按临时配置位置猜。
        winConfig.build.frontendDist=String(run("wslpath",["-w",frontendRoot],{encoding:"utf8"})).trim();
        writeFileSync(cfg,JSON.stringify(winConfig,null,2)+"\n");
        const repoWin=String(run("wslpath",["-w",root],{encoding:"utf8"})).trim();
        const cfgWin=String(run("wslpath",["-w",cfg],{encoding:"utf8"})).trim();
        const scriptWin=String(run("wslpath",["-w",script],{encoding:"utf8"})).trim();
        cmdPath(scriptWin);writeFileSync(script,windowsReleaseCommands(repoWin,cfgWin,{signed:winConfig.bundle.windows.signCommand!==null}));
        const names=["RAYBEND_VERSION","RAYBEND_CHANNEL","RAYBEND_BUILD_TIME","RAYBEND_GIT_HASH","RAYBEND_DIRTY","RAYBEND_UPDATER_PUBLIC_KEY","RAYBEND_DISTRIBUTION","TAURI_SIGNING_PRIVATE_KEY","TAURI_SIGNING_PRIVATE_KEY_PASSWORD"];
        const windowsEnv=windowsBuildEnv({...Object.fromEntries(names.map(name=>[name,buildEnv[name]])),CARGO_TARGET_DIR:"C:\\rb-target\\raybend-release"},names);
        const privateKeyFile=env.TAURI_SIGNING_PRIVATE_KEY;
        if(privateKeyFile && existsSync(privateKeyFile) && statSync(privateKeyFile).isFile())windowsEnv.TAURI_SIGNING_PRIVATE_KEY=String(run("wslpath",["-w",resolve(privateKeyFile)],{encoding:"utf8"})).trim();
        run("cmd.exe",["/d","/c",scriptWin],{cwd:root,stdio:"inherit",env:windowsEnv});
        run("pnpm",["check:win"],{cwd:root,stdio:"inherit",env:{...buildEnv,WIN_DIST:frontendRoot,WIN_EXE:"/mnt/c/rb-target/raybend-release/release/raybend-desktop.exe",WIN_WORKER_MODE:"self"}});
        finalize({argv:["/mnt/c/rb-target/raybend-release/release/bundle","--manifest",join(frontendRoot,"raybend-build.json"),"--out",releaseOut,...(request.unsigned?["--allow-unsigned"]:[]),...(request.withUpdater?["--base-url",`https://github.com/C-Thun/raybend/releases/download/v${plan.targetVersion}/`]:[])],run,log,selectVersion:plan.targetVersion,requiredTargets:winConfig.bundle.targets});
        log(`本地准备完成；由崔总真机验收后执行：pnpm release:publish ${relative(root,releaseOut)} --execute`);
      }
    }
  } catch(error) {
    try {rollback();} catch(conflict){throw new AggregateError([error,conflict],"构建失败；保留外部版本修改，需核对");}
    throw new Error(`构建失败，已恢复本次版本写入；重试前重建前端：${error.message}`,{cause:error});
  }
  if(request.skipBuild)log("版本准备完成；未构建，不是可发布产物");
  if(!request.windows)for(const command of plan.humanCommands)log(`由崔总执行：${command}`);
  return plan;
}
if(process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try {runRelease();} catch(error){console.error(`✗ ${error.message}`);process.exitCode=1;}
}
