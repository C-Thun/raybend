import { readFileSync, readdirSync, lstatSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { releaseMetadata } from './release-artifacts.mjs';
import { sha256 } from './release-files.mjs';

/** 只核对本地待上传目录和生成命令；永不调用 git push 或 gh。 */
export function githubReleasePlan(directory) {
  const root = resolve(directory);
  const files = readdirSync(root);
  for (const name of files) {
    if (!lstatSync(join(root, name)).isFile()) throw new Error('待上传目录只能包含普通文件，不能含符号链接/目录');
  }
  const index = JSON.parse(readFileSync(join(root, 'release-index.json'), 'utf8'));
  if (!['unsigned', 'verified'].includes(index.authenticode))throw new Error('非法发布者签名状态');
  if (!['release', 'beta'].includes(index.channel) || index.dirty !== false ||
      !/^[a-f0-9]{7,40}$/.test(index.gitHash ?? '')) {
    throw new Error('GitHub 公开发行要求正式/预览通道、干净来源与有效 gitHash；test 包不能上传');
  }
  const manifest = JSON.parse(readFileSync(join(root, 'raybend-build.json'), 'utf8'));
  for (const key of ['version', 'channel', 'gitHash', 'dirty', 'builtAt']) {
    if (manifest[key] !== index[key]) throw new Error(`构建与发布索引不一致：${key}`);
  }
  // 在读索引指定的文件之前先复用原契约验证文件名，避免目录穿越。
  releaseMetadata({ manifest, artifacts: index.artifacts, unsigned: index.authenticode === 'unsigned' });
  const updateName = index.channel === 'beta' ? 'beta.json' : 'latest.json';
  const artifacts = index.artifacts.map(file => {
    const bytes = readFileSync(join(root, file.name));
    if (sha256(bytes) !== file.sha256 || bytes.length !== file.bytes) throw new Error(`安装器已改变：${file.name}`);
    return { ...file, signature: files.includes(file.name + '.sig') ? readFileSync(join(root, file.name + '.sig'), 'utf8') : undefined };
  });
  const tag = `v${index.version}`;
  const baseUrl = `https://github.com/C-Thun/raybend/releases/download/${tag}/`;
  const metadata = releaseMetadata({ manifest, artifacts, baseUrl, withUpdater: files.includes(updateName), unsigned: index.authenticode === 'unsigned' });
  if (readFileSync(join(root, 'SHA256SUMS'), 'utf8') !== metadata.checksums) throw new Error('校验和与安装器索引不一致');
  if (metadata.update && JSON.stringify(JSON.parse(readFileSync(join(root, updateName), 'utf8'))) !== JSON.stringify(metadata.update)) {
    throw new Error('更新清单与最终安装器/签名/下载地址不一致');
  }
  const assets = ['SHA256SUMS', 'release-index.json', 'raybend-build.json'];
  for (const file of artifacts) {
    assets.push(file.name);
    if (file.signature !== undefined) assets.push(file.name + '.sig');
  }
  if (metadata.update) assets.push(updateName);
  const expected = new Set([...assets, 'RELEASE-NOTES.md']);
  if (files.some(name => !expected.has(name))) throw new Error('待上传目录含旧清单或未知文件，请使用新的输出目录');
  if (!readFileSync(join(root, 'RELEASE-NOTES.md'), 'utf8').trim()) throw new Error('发布说明不能为空');
  return { tag, gitHash: index.gitHash, version: index.version, prerelease: index.channel === 'beta', assets: assets.map(name => join(root, name)), notes: join(root, 'RELEASE-NOTES.md') };
}

