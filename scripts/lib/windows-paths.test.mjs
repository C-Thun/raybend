import { test } from "node:test";
import assert from "node:assert/strict";
import { discoverWindowsReleasePaths, toWindowsPath, toHostPath, runWindowsReleaseBatch } from "./windows-paths.mjs";

function bridge({ base = "E:\\用户 空格\\raybend\\build", drive = "E:\\", type = "Fixed", ready = true, mounted = true, roundtrip = true } = {}) {
  const calls = [];
  const run = (command, args) => {
    calls.push([command, ...args]);
    if (command === "powershell.exe") return JSON.stringify({ base, drive, type, ready });
    assert.equal(command, "wslpath");
    if (args[0] === "-u") return args[1].replace(/^E:\\/, "/windows/disks/e/").replaceAll("\\", "/");
    if (args[1].startsWith("/windows/disks/e/")) return roundtrip ? args[1].replace("/windows/disks/e/", "E:\\").replaceAll("/", "\\") : "\\\\wsl.localhost\\Linux\\bad";
    return "\\\\wsl.localhost\\自定义发行版" + args[1].replaceAll("/", "\\");
  };
  return { run, calls, platform: "linux", cwd: "/home/开发/repo", stat: () => { if (!mounted) throw new Error("not mounted"); return { isDirectory: () => true }; } };
}

test("自动发现 Windows 用户目录、非 C 盘和自定义挂载点，双向转换一致", () => {
  const env = bridge(), paths = discoverWindowsReleasePaths(env);
  assert.equal(paths.windowsBase, "E:\\用户 空格\\raybend\\build");
  assert.equal(paths.hostBase, "/windows/disks/e/用户 空格/raybend/build");
  assert.equal(paths.windowsSource, "E:\\用户 空格\\raybend\\build\\source");
  assert.equal(paths.hostTarget, "/windows/disks/e/用户 空格/raybend/build/target");
  const script = Buffer.from(env.calls[0].at(-1), "base64").toString("utf16le");
  assert.match(script, /GetFolderPath\('LocalApplicationData'\)/);
  assert.equal(env.calls.some(call => call[0] === "cmd.exe"), false);
});

test("指定目录接受 Windows 盘符、正斜线或挂载路径；空格/中文/单引号不损坏", () => {
  for (const directory of ["E:\\用户 空格\\构建's", "E:/用户 空格/构建's", "/windows/disks/e/用户 空格/构建's"]) {
    const env = bridge({ base: "E:\\用户 空格\\构建's" });
    assert.equal(discoverWindowsReleasePaths({ ...env, directory }).windowsBase, "E:\\用户 空格\\构建's");
    const command = env.calls.find(call => call[0] === "powershell.exe");
    const script = Buffer.from(command.at(-1), "base64").toString("utf16le");
    assert.match(script, /构建''s/);
  }
  assert.equal(discoverWindowsReleasePaths(bridge({ base: "E:\\build\\" })).windowsBase, "E:\\build");
});

test("Linux 源码路径可转 UNC 给普通 Windows 工具，但不能当 MSI 构建目录", () => {
  const env = bridge();
  assert.equal(toWindowsPath("keys/私钥.txt", env), "\\\\wsl.localhost\\自定义发行版\\home\\开发\\repo\\keys\\私钥.txt");
  for (const directory of ["/home/开发/build", "\\\\wsl.localhost\\Linux\\home\\build", "\\\\server\\share\\build"]) {
    assert.throws(() => discoverWindowsReleasePaths({ ...env, directory }), /本地盘/);
  }
  assert.equal(toHostPath("E:\\私钥.txt", env), "/windows/disks/e/私钥.txt");
  assert.equal(toHostPath("./keys/key.txt", env), "/home/开发/repo/keys/key.txt");
});

test("拒绝不存在的挂载、错误回译、网络映射盘、离线盘和盘根目录", () => {
  for (const options of [{ mounted: false }, { roundtrip: false }, { type: "Network" }, { ready: false }, { base: "E:\\" }]) {
    assert.throws(() => discoverWindowsReleasePaths(bridge(options)), /挂载|不可用|盘根/);
  }
  assert.throws(() => discoverWindowsReleasePaths({ ...bridge(), directory: "" }), /路径为空/);
  assert.throws(() => toWindowsPath("E:relative", bridge()), /盘符绝对路径/);
  assert.throws(() => toWindowsPath("a\nb", bridge()), /控制字符/);
  assert.throws(() => toWindowsPath("./dir", { run: () => "bad", platform: "linux" }), /Windows 绝对路径/);
  assert.throws(() => toHostPath("E:\\dir", { run: () => "relative", platform: "linux" }), /绝对路径/);
  assert.throws(() => discoverWindowsReleasePaths({ run: () => { throw Error("interop disabled"); } }), /互操作/);
});

test("路径转换在原生 Windows 上保持盘符路径，批处理使用本地 cwd 与固定相对命令", () => {
  const run = () => { throw new Error("不应调用 wslpath"); };
  assert.equal(toWindowsPath(".\\中文", { run, platform: "win32", cwd: "F:\\repo" }), "F:\\repo\\中文");
  assert.equal(toHostPath("F:/构建/source", { run, platform: "win32" }), "F:\\构建\\source");
  runWindowsReleaseBatch("/windows/disks/e/用户 空格/build/source", { env: { KEY: "value" }, run: (command, args, options) => {
    assert.equal(command, "cmd.exe"); assert.deepEqual(args, ["/d", "/c", ".release\\build.cmd"]);
    assert.equal(options.cwd, "/windows/disks/e/用户 空格/build/source"); assert.equal(options.env.KEY, "value");
  } });
});
