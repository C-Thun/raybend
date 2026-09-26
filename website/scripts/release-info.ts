import { readFileSync } from 'node:fs';
import { parseTag, type ReleaseInfo } from '../src/data/release.ts';

/** GitHub REST / release 事件只经这一层适配；草稿不能出现在静态官网。 */
export function githubReleaseInfo(value: unknown): ReleaseInfo | null {
  if (!value || typeof value !== 'object') return null;
  const data = value as Record<string, unknown>;
  const tag = typeof data.tag_name === 'string' ? parseTag(data.tag_name) : undefined;
  if (!tag || tag.split('+')[0]!.includes('-') || data.draft !== false || data.prerelease !== false) return null;
  return {
    tag,
    url: typeof data.html_url === 'string' ? data.html_url : undefined,
    publishedAt: typeof data.published_at === 'string' ? data.published_at : undefined,
    draft: false,
    prerelease: false,
    assets: (Array.isArray(data.assets) ? data.assets : []).flatMap((asset: Record<string, unknown>) =>
      asset && typeof asset.name === 'string' && typeof asset.browser_download_url === 'string'
        ? [{ name: asset.name, size: typeof asset.size === 'number' ? asset.size : undefined, url: asset.browser_download_url }]
        : []),
  };
}

/** 本地默认不联网；published 事件使用完整事件资产，避免 API 时序/限流。 */
export async function fetchReleaseInfo({ env = process.env, request = fetch, read = (path: string) => readFileSync(path, 'utf8') } = {}): Promise<ReleaseInfo | null> {
  const tag = env.RAYBEND_TAG?.trim();
  if (tag && !parseTag(tag)) throw new Error('非法 RAYBEND_TAG');
  if (env.GITHUB_EVENT_NAME === 'release' && env.GITHUB_EVENT_PATH) {
    const event = JSON.parse(read(env.GITHUB_EVENT_PATH)) as { release?: unknown };
    const info = githubReleaseInfo(event.release);
    if (!info || (tag && info.tag !== tag)) throw new Error('官网发布事件必须是所选 tag 的已公开正式版本');
    return info;
  }
  if (!tag && env.CI !== 'true' && env.RAYBEND_FETCH_RELEASE !== '1') return null;
  const api = 'https://api.github.com/repos/C-Thun/raybend/releases';
  const endpoint = tag ? `${api}/tags/${encodeURIComponent(tag)}` : `${api}/latest`;
  const token = env.GITHUB_TOKEN ?? env.GH_TOKEN;
  const response = await request(endpoint, {
    headers: { accept: 'application/vnd.github+json', 'user-agent': 'raybend-website-build', ...(token ? { authorization: `Bearer ${token}` } : {}) },
    signal: AbortSignal.timeout(8000),
  });
  if (response.status === 404 && !tag) return null; // 尚无正式发行是正常状态。
  if (!response.ok) throw new Error(`GitHub release fetch failed: HTTP ${response.status}`);
  const info = githubReleaseInfo(await response.json());
  if (!info || (tag && info.tag !== tag)) throw new Error('GitHub 响应不是所选的已公开正式版');
  return info;
}
