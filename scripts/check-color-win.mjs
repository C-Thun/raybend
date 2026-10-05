/** Windows hardware/offscreen diagnostics; no release or GUI interaction. */
import {spawnSync,execFileSync} from "node:child_process";
import {dirname,resolve} from "node:path";
import {fileURLToPath} from "node:url";
import {windowsBuildEnv} from "./lib/dav1d-win.mjs";

const root=resolve(dirname(fileURLToPath(import.meta.url)),"..");
const repository=execFileSync("wslpath",["-w",root],{encoding:"utf8"}).trim();
if(/[&|<>\r\n"]/.test(repository))throw new Error("Unsupported Windows repository path");
const target="C:\\rb-target\\raybend";
const profiles=process.argv.slice(2).map(path=>execFileSync("wslpath",["-w",resolve(path)],{encoding:"utf8"}).trim());
const build=spawnSync("cmd.exe",["/c",`pushd ${repository} & cargo build -p raybend --example color-offscreen --example color-draw-cost --example color-memory`],
  {cwd:root,env:windowsBuildEnv({CARGO_TARGET_DIR:target}),stdio:"inherit"});
if(build.status!==0)process.exit(build.status??1);
for(const name of ["color-offscreen","color-draw-cost"]) {
  console.log(`\n${name} — Windows DX12`);
  const result=spawnSync(`/mnt/c/rb-target/raybend/debug/examples/${name}.exe`,profiles,
    {cwd:root,env:windowsBuildEnv({WGPU_BACKEND:"dx12"}),stdio:"inherit"});
  if(result.status!==0)process.exit(result.status??1);
}
