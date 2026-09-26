import { describe, it, expect } from 'vitest';
import { fetchReleaseInfo, githubReleaseInfo } from './release-info.ts';
import { resolveRelease, parseTag, pickInstaller } from '../src/data/release.ts';
const release = { tag_name: 'v1.2.3', draft: false, prerelease: false, published_at: '2026-09-27T00:00:00Z', assets: [{ name: 'RayBend_1.2.3_x64-setup.exe', browser_download_url: 'https://github.com/C-Thun/raybend/releases/download/v1.2.3/RayBend_1.2.3_x64-setup.exe' }] };
describe('官网构建期 Release 接线', () => {
  it('本地构建不联网；published 事件直接得到已上传的直链', async () => {
    const request = (() => { throw new Error('不该联网'); }) as typeof fetch;
    expect(await fetchReleaseInfo({ env: {}, request })).toBeNull();
    const info = await fetchReleaseInfo({ env: { RAYBEND_TAG: 'v1.2.3', GITHUB_EVENT_NAME: 'release', GITHUB_EVENT_PATH: 'event' }, request, read: () => JSON.stringify({ release }) });
    expect(resolveRelease(info)).toMatchObject({ state: 'available', downloadUrl: release.assets[0]!.browser_download_url });
  });
  it('草稿、预发布、缺少公开标记与不匹配事件不伪造可用版本', async () => {
    for (const changes of [{ draft: true }, { prerelease: true }, { draft: undefined }, { tag_name: 'v1.2.3-beta.1' }]) expect(githubReleaseInfo({ ...release, ...changes })).toBeNull();
    expect(resolveRelease({ tag: 'v1.2.3-beta.1' }).state).toBe('pending');
    expect(resolveRelease({ tag: 'v1.2.3', draft: true }).state).toBe('pending');
    await expect(fetchReleaseInfo({ env: { RAYBEND_TAG: 'v2.0.0', GITHUB_EVENT_NAME: 'release', GITHUB_EVENT_PATH: 'event' }, read: () => JSON.stringify({ release }) })).rejects.toThrow('所选');
  });
  it('尚无版本 404 回 pending；网络/API 错误停止部署，保留已部署网站', async () => {
    const request = (async () => new Response('', { status: 404 })) as typeof fetch;
    expect(await fetchReleaseInfo({ env: { CI: 'true' }, request })).toBeNull();
    await expect(fetchReleaseInfo({ env: { RAYBEND_TAG: 'v1.2.3' }, request })).rejects.toThrow('404');
    await expect(fetchReleaseInfo({ env: { CI: 'true' }, request: (async () => new Response('', { status: 500 })) as typeof fetch })).rejects.toThrow('500');
    const info = await fetchReleaseInfo({ env: { CI: 'true' }, request: (async () => Response.json(release)) as typeof fetch });
    expect(info?.tag).toBe('v1.2.3');
  });
  it('非法版本、别的版本/平台/裸 exe/非 GitHub URL 不成为下载直链', () => {
    for (const tag of ['v01.2.3', 'v1.2.3-', 'v1.2.3-beta..1', 'v1.2.3-beta.01']) expect(parseTag(tag)).toBeUndefined();
    for (const asset of [
      { name: 'RayBend_1.2.3_x64-setup.exe', url: 'http://github.com/C-Thun/raybend/releases/download/v1.2.3/RayBend_1.2.3_x64-setup.exe' },
      { name: 'raybend-desktop.exe', url: release.assets[0]!.browser_download_url },
      { name: 'RayBend_1.2.3_x64-setup.exe', url: 'https://evil.test/setup.exe' },
    ]) expect(pickInstaller([asset])).toBeUndefined();
    expect(resolveRelease({ tag: 'v2.0.0', assets: [{ name: release.assets[0]!.name, url: release.assets[0]!.browser_download_url }] })).toMatchObject({ downloadUrl: 'https://github.com/C-Thun/raybend/releases/tag/v2.0.0' });
  });
});
