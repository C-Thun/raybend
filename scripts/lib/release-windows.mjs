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
      // 正式版也只出 NSIS（2026-09-27 崔总定）：MSI 那条被「32 位 light.exe 读不了 WSL 路径
      // + tauri 的 bundle.resources 无法表达结对路径」结构性挡住。恢复路线登记在 FUTURE.md，
      // 现场记录与实测矩阵见 implementations/2026-09-27_release-msi-wix-light-path.md。
      targets: ["nsis"],
      createUpdaterArtifacts: env.withUpdater === true,
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
export function windowsReleaseCommands(repo, config, {signed=false}={}) {
  return [
    "@echo off",
    `pushd ${cmdPath(repo)} || exit /b 1`,
    "where cargo-tauri >nul 2>&1 || (echo Install tauri-cli with cargo before packaging. & exit /b 1)",
    ...(signed ? ["where signtool.exe >nul 2>&1 || (echo Add Windows SDK signtool to PATH before signing. & exit /b 1)"] : []),
    "cargo build -p raybend --release --locked || exit /b 1",
    `cargo tauri build --no-bundle --features custom-protocol --config ${cmdPath(config)} -- --locked || exit /b 1`,
    `cargo tauri bundle --config ${cmdPath(config)} || exit /b 1`,
    "popd",
  ].join("\r\n")+"\r\n";
}
