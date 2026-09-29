/** WSL ↔ Windows 路径只在此转换；不猜 /mnt、盘符、用户名或发行版名称。 */
import { execFileSync } from "node:child_process";
import { statSync } from "node:fs";
import { resolve, win32 } from "node:path";
import { cmdPath } from "./release-windows.mjs";

const windowsAbsolute = value => /^[A-Za-z]:[\\/]/.test(value) || /^\\\\[^\\]+\\[^\\]+/.test(value);
const localAbsolute = value => /^[A-Za-z]:\\/.test(value);

export function windowsPowerShell(script, { run = execFileSync, ...options } = {}) {
  const code = "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); " + script;
  return run("powershell.exe", ["-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand", Buffer.from(code, "utf16le").toString("base64")], { encoding: "utf8", ...options });
}

export function toWindowsPath(value, { run = execFileSync, cwd = process.cwd(), platform = process.platform } = {}) {
  if (typeof value !== "string" || !value || /[\u0000-\u001f]/.test(value)) throw new Error("路径为空或包含控制字符");
  if (windowsAbsolute(value)) return win32.normalize(value);
  if (/^[A-Za-z]:/.test(value)) throw new Error(`不支持依赖当前盘目录的路径，请用盘符绝对路径：${value}`);
  if (platform === "win32") return win32.resolve(cwd, value);
  const result = String(run("wslpath", ["-w", resolve(cwd, value)], { encoding: "utf8" })).trim();
  if (!windowsAbsolute(result)) throw new Error(`wslpath 未返回 Windows 绝对路径：${value}`);
  return win32.normalize(result);
}

export function toHostPath(value, { run = execFileSync, cwd = process.cwd(), platform = process.platform } = {}) {
  if (platform === "win32") return toWindowsPath(value, { run, cwd, platform });
  if (!windowsAbsolute(value)) return resolve(cwd, value);
  const result = String(run("wslpath", ["-u", win32.normalize(value)], { encoding: "utf8" })).trim();
  if (!result.startsWith("/")) throw new Error(`wslpath 未返回已挂载的绝对路径：${value}`);
  return result;
}

/** 只读探测：Windows 本地应用目录默认值，可用 Windows 或 WSL 路径覆盖。 */
export function discoverWindowsReleasePaths({ directory, run = execFileSync, cwd = process.cwd(), platform = process.platform, stat = statSync } = {}) {
  const context = { run, cwd, platform };
  const requested = directory === undefined ? undefined : toWindowsPath(directory, context);
  if (requested && !localAbsolute(requested)) throw new Error("Windows 构建目录必须位于本地盘；WSL Linux / UNC / 网络目录不能交给 WiX");
  // 单引号字面量转义，EncodedCommand 避免跨 WSL 的命令行引号损坏与 Unicode 编码丢失。
  const literal = value => `'${value.replaceAll("'", "''")}'`;
  const base = requested ? literal(requested) : "[IO.Path]::Combine([Environment]::GetFolderPath('LocalApplicationData'),'raybend','build')";
  let info;
  try {
    info = JSON.parse(String(windowsPowerShell(`$base=${base}; $drive=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($base)); @{base=$base; drive=$drive.Name; type=$drive.DriveType.ToString(); ready=$drive.IsReady} | ConvertTo-Json -Compress`, { run })).trim());
  } catch (error) {
    throw new Error(`无法探测 Windows 构建盘；请检查 WSL 互操作及 powershell.exe：${error.message}`, { cause: error });
  }
  if (!localAbsolute(info.base ?? "") || !localAbsolute(info.drive ?? "") || !["Fixed", "Removable"].includes(info.type) || info.ready !== true) {
    throw new Error("Windows 构建盘不可用或属于网络盘，请用 --win-dir 指向已挂载的本地盘目录");
  }
  const windowsBase = win32.resolve(info.base);
  if (windowsBase === win32.parse(windowsBase).root) throw new Error("--win-dir 必须指向专用构建目录，不能直接使用盘根目录");
  cmdPath(windowsBase);
  const hostBase = toHostPath(windowsBase, context), hostDrive = toHostPath(info.drive, context);
  let mounted = false;
  try { mounted = stat(hostDrive).isDirectory(); } catch { /* Windows 存在但 WSL 未挂载。 */ }
  if (!mounted || win32.resolve(toWindowsPath(hostBase, context)).toLowerCase() !== windowsBase.toLowerCase()) {
    throw new Error(`Windows 盘 ${info.drive} 尚未正确挂载到 WSL，或双向路径转换不一致：${hostBase}`);
  }
  return {
    windowsBase, hostBase, hostDrive,
    windowsSource: win32.join(windowsBase, "source"), hostSource: toHostPath(win32.join(windowsBase, "source"), context),
    windowsTarget: win32.join(windowsBase, "target"), hostTarget: toHostPath(win32.join(windowsBase, "target"), context),
  };
}

/** 在 Windows 本地工作目录里运行批处理，cmd 参数只有固定文件名，目录空格/中文不经 shell。 */
export function runWindowsReleaseBatch(hostSource, { run = execFileSync, env } = {}) {
  return run("cmd.exe", ["/d", "/c", ".release\\build.cmd"], { cwd: hostSource, stdio: "inherit", env });
}
