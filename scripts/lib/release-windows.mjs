import { windowsVersion } from "../../src/lib/release-plan.ts";
export function windowsReleaseConfig(plan, env) {
  windowsVersion(plan.targetVersion);
  const unsigned = env.unsigned === true;
  const thumb = env.certThumbprint ?? "";
  if (!unsigned && !/^[a-fA-F0-9]{40}$/.test(thumb)) throw new Error("Windows 签名需要 RAYBEND_SIGN_CERT_SHA1（Windows 证书存储中的 40 位指纹）；无签名显式 --unsigned");
  if (env.withUpdater && (!env.updaterPublicKey || !env.updaterPrivateKey)) throw new Error("更新产物需要 RAYBEND_UPDATER_PUBLIC_KEY 与 TAURI_SIGNING_PRIVATE_KEY；私钥只能由崔总配置");
  const timestamp = env.timestamp ?? "https://timestamp.digicert.com";
  let parsed; // 解析失败也走同一条报错，不让裸 TypeError 冒到崔总面前。
  try { parsed = new URL(timestamp); } catch { parsed = null; }
  if(!parsed || parsed.protocol!=="https:" || parsed.username || parsed.password) throw new Error("时间戳服务须使用无凭据 HTTPS URL");
  return {
    plugins: {updater: {pubkey:env.updaterPublicKey ?? "",endpoints:[],windows:{installMode:"basicUi"}}},
    build: { beforeBuildCommand: null, frontendDist: env.frontendDist },
    bundle: {
      targets: [...plan.windowsTargets],
      createUpdaterArtifacts: env.withUpdater === true,
      /*
       * 崩溃助手（W4）：Tauri 的 sidecar 约定 —— 写「名字」，打包时去找
       * `src-tauri/binaries/raybend-crashpad-<target-triple>.exe`（由 `windowsReleaseCommands`
       * 从 cargo 产物拷过来），装完与主程序**同目录**（这正是主程序找它的位置）。
       *
       * ❗只写在发版配置里，不写进 `tauri.conf.json`：后者在 `tauri dev` / 手工 build 时也会被读，
       * 而那时 `binaries/` 里没有文件 → 构建直接报错。
       */
      externalBin: ["binaries/raybend-crashpad"],
      windows: {
        allowDowngrades: false,
        signCommand: unsigned ? null : { cmd:"signtool.exe",args:["sign","/fd","SHA256","/td","SHA256","/tr",timestamp,"/sha1",thumb,"%1"] },
      },
    },
  };
}
/** Windows 原生命令写入 .cmd 文件，路径以双引号围住，拒绝 shell 注入字符。 */
export function cmdPath(value) {
  if (!value || /[\r\n"%&|<>^!]/.test(value)) throw new Error("Windows 构建路径包含不支持的 cmd 字符");
  return `"${value}"`;
}
export function windowsReleaseCommands(repo, config, {signed=false,features=["custom-protocol"]}={}) {
  if(features.some(f=>!/[a-z0-9-]+/.test(f))||features.some(f=>!/^[-a-z0-9]+$/.test(f)))throw new Error("非法构建 feature");
  const featureArgs=features.join(",");
  // WiX 32 位 light.exe 必须从本地盘读资源；禁止重新落回 UNC / pushd 映射盘。
  for (const path of [repo, config]) {
    if (!/^[A-Za-z]:\\/.test(path)) throw new Error("Windows 发行构建必须使用本地盘绝对路径");
  }
  return [
    "@echo off",
    "chcp 65001 >nul",
    `cd /d ${cmdPath(repo)} || exit /b 1`,
    "where cargo-tauri >nul 2>&1 || (echo Install tauri-cli with cargo before packaging. & exit /b 1)",
    ...(signed ? ["where signtool.exe >nul 2>&1 || (echo Add Windows SDK signtool to PATH before signing. & exit /b 1)"] : []),
    "cargo build -p raybend --release --locked || exit /b 1",
    /*
     * 崩溃助手：先构建，再拷成 Tauri 的 sidecar 命名（`<name>-<triple>.exe`）。
     * 缺了它 bundle 会直接失败——所以这里拷贝失败就当场停，不留到打包时才报。
     */
    "cargo build -p raybend-crashpad --release --locked || exit /b 1",
    "if not exist src-tauri\\binaries mkdir src-tauri\\binaries",
    "copy /y \"%CARGO_TARGET_DIR%\\release\\raybend-crashpad.exe\" \"src-tauri\\binaries\\raybend-crashpad-x86_64-pc-windows-msvc.exe\" >nul || (echo Missing raybend-crashpad build. & exit /b 1)",
    `cargo tauri build --no-bundle --features ${featureArgs} --config ${cmdPath(config)} -- --locked || exit /b 1`,
    `cargo tauri bundle -vv --features ${featureArgs} --config ${cmdPath(config)} || exit /b 1`,
    "exit /b 0",
  ].join("\r\n")+"\r\n";
}
