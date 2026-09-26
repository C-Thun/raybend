import { describe, expect, it } from 'vitest';
import {
  formatBytes,
  parseTag,
  pickInstaller,
  resolveRelease,
  type ReleaseInfo,
} from './release.ts';
import { REPO_URL } from './site.ts';

/** 一个「正式版 + NSIS 安装包」的完整发布信息 */
const FULL_RELEASE: ReleaseInfo = {
  tag: 'v0.1.0',
  url: 'https://github.com/C-Thun/raybend/releases/tag/v0.1.0',
  publishedAt: '2026-10-01T09:30:00Z',
  prerelease: false,
  assets: [
    { name: 'RayBend_0.1.0_x64-setup.exe.sig', size: 400, url: 'https://example.com/sig' },
    { name: 'RayBend_0.1.0_x64-setup.exe', size: 12_582_912, url: 'https://github.com/C-Thun/raybend/releases/download/v0.1.0/RayBend_0.1.0_x64-setup.exe' },
    { name: 'RayBend_0.1.0_x64.zip', size: 11_000_000, url: 'https://example.com/portable.zip' },
  ],
};

describe('parseTag', () => {
  it('接受带 v 与不带 v 的版本号', () => {
    expect(parseTag('v1.2.3')).toBe('v1.2.3');
    expect(parseTag('1.2.3')).toBe('1.2.3');
    expect(parseTag('  v0.1.0-beta.2  ')).toBe('v0.1.0-beta.2');
  });

  it('拒绝不像版本号的 tag', () => {
    expect(parseTag(undefined)).toBeUndefined();
    expect(parseTag(null)).toBeUndefined();
    expect(parseTag('')).toBeUndefined();
    expect(parseTag('nightly')).toBeUndefined();
    expect(parseTag('v1.2')).toBeUndefined();
    expect(parseTag('website-v1')).toBeUndefined();
  });
});

describe('formatBytes', () => {
  it('按量级给单位与一位小数', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2.0 KB');
    expect(formatBytes(12_582_912)).toBe('12.0 MB');
    expect(formatBytes(3 * 1024 ** 3)).toBe('3.0 GB');
  });

  it('非法值返回 undefined', () => {
    expect(formatBytes(0)).toBeUndefined();
    expect(formatBytes(-5)).toBeUndefined();
    expect(formatBytes(Number.NaN)).toBeUndefined();
    expect(formatBytes(undefined)).toBeUndefined();
  });
});

describe('pickInstaller', () => {
  it('优先 exe，其次是 msi / zip，签名文件不算', () => {
    expect(pickInstaller(FULL_RELEASE.assets)?.name).toBe('RayBend_0.1.0_x64-setup.exe');
    expect(pickInstaller([{ name: 'a.zip', url: 'z' }])).toBeUndefined();
    expect(pickInstaller([{ name: 'RayBend_0.1.0_x64_en-US.msi', url: 'https://github.com/C-Thun/raybend/releases/download/v0.1.0/RayBend_0.1.0_x64_en-US.msi' }, { name: 'a.exe.sig', url: 's' }])?.name).toBe(
      'RayBend_0.1.0_x64_en-US.msi',
    );
  });

  it('没有资产或资产没有直链时返回 undefined', () => {
    expect(pickInstaller(undefined)).toBeUndefined();
    expect(pickInstaller([])).toBeUndefined();
    expect(pickInstaller([{ name: 'a.exe' }])).toBeUndefined();
  });
});

describe('resolveRelease', () => {
  it('没有发布信息时是 pending，按钮指向发布列表页', () => {
    const view = resolveRelease(null);
    expect(view.state).toBe('pending');
    expect(view.releaseUrl).toBe(`${REPO_URL}/releases`);
  });

  it('正式版：给出安装包直链、版本号（去掉 v）、体积与时间', () => {
    const view = resolveRelease(FULL_RELEASE);
    expect(view).toEqual({
      state: 'available',
      version: '0.1.0',
      tag: 'v0.1.0',
      releaseUrl: FULL_RELEASE.url,
      downloadUrl: 'https://github.com/C-Thun/raybend/releases/download/v0.1.0/RayBend_0.1.0_x64-setup.exe',
      fileName: 'RayBend_0.1.0_x64-setup.exe',
      sizeLabel: '12.0 MB',
      publishedAt: '2026-10-01T09:30:00Z',
    });
  });

  it('有版本但没有可识别资产时退化为发布页', () => {
    const view = resolveRelease({ tag: 'v0.2.0', assets: [{ name: 'notes.txt', url: 'n' }] });
    expect(view.state).toBe('available');
    if (view.state !== 'available') throw new Error('unreachable');
    expect(view.downloadUrl).toBe(`${REPO_URL}/releases/tag/v0.2.0`);
    expect(view.sizeLabel).toBeUndefined();
  });

  it('预发布版一律不上官网', () => {
    expect(resolveRelease({ ...FULL_RELEASE, prerelease: true }).state).toBe('pending');
  });

  it('tag 不合法时当作还没发布', () => {
    expect(resolveRelease({ tag: 'nightly' }).state).toBe('pending');
  });
});
