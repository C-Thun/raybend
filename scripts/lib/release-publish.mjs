import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { githubReleasePlan } from './release-upload.mjs';
import { sha256, versionEdits, RELEASE_SOURCE_FILES } from './release-files.mjs';

export { RELEASE_SOURCE_FILES } from './release-files.mjs';

/** 发布副作用只在崔总显式 --execute 时执行；run 注入用于完全离线的合成回归。 */
export function publishRelease({ root, directory, execute = false, run = execFileSync, log = console.log, wait = () => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 2000) }) {
  const plan = githubReleasePlan(directory);
  const manifest = JSON.parse(readFileSync(join(directory, 'raybend-build.json'), 'utf8'));
  const command = (cmd, args, extra = {}) => run(cmd, args, { cwd: root, encoding: 'utf8', maxBuffer: 64*1024*1024, stdio: ['ignore', 'pipe', 'pipe'], ...extra });
  const git = (...args) => String(command('git', args)).trim();
  const gh = (...args) => command('gh', args);
  const base = git('rev-parse', `${plan.gitHash}^{commit}`);
  let head = git('rev-parse', 'HEAD');
  const branch = git('symbolic-ref', '--short', 'HEAD');
  if (branch !== 'master') throw new Error('当前发行流程要求 master 分支；不自动切换或合并分支');
  if (!/^(git@github\.com:C-Thun\/raybend(?:\.git)?|https:\/\/github\.com\/C-Thun\/raybend(?:\.git)?)$/i.test(git('remote', 'get-url', 'origin'))) throw new Error('origin 必须是 C-Thun/raybend，避免推送到错误仓库');
  if (head !== base && git('rev-list', '--parents', '-n', '1', 'HEAD') !== `${head} ${base}`) throw new Error('HEAD 已离开构建来源；请重新构建，不能发布并行提交');
  const sourceFiles = manifest.sourceFiles;
  for (const path of RELEASE_SOURCE_FILES) {
    if (!sourceFiles || !/^[a-f0-9]{64}$/.test(sourceFiles[path] ?? '') || sha256(readFileSync(join(root, path))) !== sourceFiles[path]) throw new Error(`源码已改变或缺构建快照：${path}`);
  }
  const expected = versionEdits(root, plan.version, path => String(command('git', ['show', `${base}:${relative(root, path).replaceAll('\\', '/')}`])));
  for (const edit of expected) {
    if (readFileSync(edit.path, 'utf8') !== edit.after) throw new Error(`版本文件含升版以外的改动：${relative(root, edit.path)}`);
  }
  const license = JSON.parse(readFileSync(join(root, 'public/legal/third-party.json'), 'utf8'));
  if (license.locks?.cargo !== sha256(readFileSync(join(root, 'Cargo.lock'))) || license.locks?.pnpm !== sha256(readFileSync(join(root, 'pnpm-lock.yaml')))) throw new Error('许可清单锁摘要不匹配');
  const statusPaths = () => String(command('git', ['status', '--porcelain=v1', '-z'])).split('\0').filter(Boolean).map(entry => {
    if (!/^[ MARC?!]{2} /.test(entry) || entry.startsWith('R') || entry[1] === 'R') throw new Error('发行期间存在重命名或未知状态');
    return entry.slice(3);
  });
  const checkParallel = () => {
    const paths = statusPaths();
    if (paths.some(path => !RELEASE_SOURCE_FILES.includes(path))) throw new Error('工作树含并行改动/未跟踪文件；请先收口，发布脚本不会提交它们');
    return paths;
  };
  const changes = checkParallel();
  if (git('diff', '--cached', '--name-only')) throw new Error('暂存区非空；请先人工处理，避免混入发布提交');
  if (!execute) {
    log(`预览：${plan.tag}；将只提交版本/许可资源，创建精确 tag，原子推送 master + 该 tag，上传草稿并核对全部字节，再公开发布。\n官网由 release:published 事件自动构建并部署 Pages。没有执行 git 写入或联网。`);
    return plan;
  }
  try {gh('auth', 'status');} catch {throw new Error('需在 WSL 安装并登录 GitHub CLI gh（https://cli.github.com/），尚未改写 git 或上传');} // 在任何 git 写入之前检查账户。
  // GH 404 才能当作“首次发布”；网络/认证错误不可吞掉。
  const readRemote = () => {
    try { return JSON.parse(String(gh('api', `repos/C-Thun/raybend/releases/tags/${plan.tag}`))); }
    catch (error) { if (/HTTP 404/.test(String(error.stderr ?? '') + error.message)) return null; throw error; }
  };
  let release = readRemote();
  if (release && (release.tag_name !== plan.tag || release.prerelease !== plan.prerelease)) throw new Error('已有 Release 通道/tag 不匹配；不覆盖');
  const verifyAssets = (remote, requireAll = false) => {
    const expectedAssets = new Map(plan.assets.map(path => [path.split(/[\\/]/).at(-1), path]));
    for (const asset of remote.assets ?? []) {
      const path = expectedAssets.get(asset.name);
      if (!path) throw new Error(`已有 Release 含未知资产，拒绝覆盖：${asset.name}`);
      if (remote.draft && asset.state==='starter' && asset.size===0)continue; // GitHub 中断上传留下的零字节占位，不当作已完成资产。
      const local = readFileSync(path);
      let digest = asset.digest;
      if (!digest) digest = 'sha256:' + sha256(command('gh', ['api', `repos/C-Thun/raybend/releases/assets/${asset.id}`, '-H', 'Accept: application/octet-stream'], { encoding: null, maxBuffer: local.length+1024*1024 }));
      if (asset.size !== local.length || digest !== 'sha256:' + sha256(local)) throw new Error(`已有远程资产与本地不符：${asset.name}`);
      expectedAssets.delete(asset.name);
    }
    if (requireAll && expectedAssets.size) throw new Error('上传尚未完整；Release 保持草稿，请重跑同一发布指令');
    return [...expectedAssets.values()];
  };
  if (release) verifyAssets(release, !release.draft);
  if (head !== base && changes.length) throw new Error('发布提交之后又有变化，拒绝二次提交');
  if (changes.length) {
    command('git', ['add', '--', ...RELEASE_SOURCE_FILES]);
    checkParallel();
    for (const path of RELEASE_SOURCE_FILES) {
      if (sha256(readFileSync(join(root,path))) !== sourceFiles[path] || sha256(command('git', ['show', `:${path}`], { encoding: null })) !== sourceFiles[path]) throw new Error(`暂存期间文件改变：${path}`);
    }
    command('git', ['commit', '-m', `chore: 发布版本 ${plan.version}`]);
    head = git('rev-parse', 'HEAD');
  }
  if (statusPaths().length) throw new Error('发布提交后出现并行改动；保留现场，尚未推送');
  const tags = git('tag', '--list', plan.tag);
  if (tags) {
    if (git('rev-parse', `${plan.tag}^{commit}`) !== head) throw new Error('已有 tag 指向其它提交，不覆盖');
  } else command('git', ['tag', '-a', plan.tag, '-m', `RayBend ${plan.version}`]);
  command('git', ['push', '--atomic', 'origin', 'HEAD:refs/heads/master', `refs/tags/${plan.tag}:refs/tags/${plan.tag}`]);
  if (!release) {
    gh('release', 'create', plan.tag, '--repo', 'C-Thun/raybend', '--verify-tag', '--draft', '--title', `RayBend ${plan.version}`, '--notes-file', plan.notes, ...(plan.prerelease ? ['--prerelease'] : []));
    release = readRemote();
    if (!release) throw new Error('创建后未找到 Release，请重跑同一指令');
  }
  if (release.draft) {
    const missing = verifyAssets(release);
    for(const asset of release.assets??[]){
      if(asset.state==='starter' && asset.size===0 && missing.some(path=>path.split(/[\\/]/).at(-1)===asset.name))gh('release','delete-asset',plan.tag,asset.name,'--repo','C-Thun/raybend','--yes');
    }
    if (missing.length) gh('release', 'upload', plan.tag, ...missing, '--repo', 'C-Thun/raybend');
    release = readRemote();
    if (!release) throw new Error('上传后 Release 不可读');
    verifyAssets(release, true);
    gh('release', 'edit', plan.tag, '--repo', 'C-Thun/raybend', '--draft=false', `--latest=${!plan.prerelease}`);
  }
  if (!plan.prerelease) {
    release=readRemote();
    let siteRun;
    for(let attempt=0;attempt<30;attempt++){
      const runs=JSON.parse(String(gh('run','list','--repo','C-Thun/raybend','--workflow','website.yml','--event','release','--limit','20','--json','databaseId,createdAt,headSha,status,conclusion')));
      siteRun=runs.find(r=>r.headSha===head && Date.parse(r.createdAt)>=Date.parse(release.published_at)-5000);
      if(siteRun)break;
      wait();
    }
    if(!siteRun)throw new Error('GitHub Release 已公开，但尚未找到官网工作流；请检查 Actions 开关/令牌，重跑发布指令不会重传包');
    if(siteRun.status==='completed' && siteRun.conclusion!=='success'){
      gh('run','rerun',String(siteRun.databaseId),'--repo','C-Thun/raybend','--failed');
      wait();
    }
    command('gh',['run','watch',String(siteRun.databaseId),'--repo','C-Thun/raybend','--exit-status'],{stdio:'inherit'});
  }
  log(`已发布：https://github.com/C-Thun/raybend/releases/tag/${plan.tag}\n安装器：https://github.com/C-Thun/raybend/releases/download/${plan.tag}/${encodeURIComponent(plan.assets.find(p=>p.toLowerCase().endsWith('.exe'))?.split(/[\\/]/).at(-1) ?? '')}\n${plan.prerelease ? '预览版不上官网；beta 更新 JSON 部署仍按 docs/release.md。' : '官网 Actions 构建/Pages 部署已通过：https://github.com/C-Thun/raybend/actions/workflows/website.yml\n下载区已由本版 Release 数据自动生成；仍需您打开官网确认访问结果。'}`);
  return plan;
}
