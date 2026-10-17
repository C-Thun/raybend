/**
 * Hero 主按钮的回归（2026-10-17 真机截图抓到的 bug）。
 *
 * 那颗「下载 Windows 版」大按钮曾经**无条件**指向 GitHub 的 release 页面 ——
 * 点下去不下载，用户还得自己到页面上找安装包；而页面底部的下载区却是文件直链。
 * 规格 §5 写明「页面两处用它：hero 主按钮 + 下载区大卡」，也就是说两处必须是
 * 同一份下载信息（有安装包就是文件直链）。这条测试锁住它，防止再退回。
 */
import { render } from '@solidjs/testing-library';
import { describe, expect, it, vi } from 'vitest';

/** v0.1.3 的真实资产形状（MSI 直链 + 同版签名文件） */
const MSI_URL =
  'https://github.com/C-Thun/raybend/releases/download/v0.1.3/RayBend_0.1.3_x64_en-US.msi';

vi.mock('../data/release.ts', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../data/release.ts')>();
  return {
    ...actual,
    // 固定为「已发布正式版、带 MSI 资产」，与线上注入的数据同形
    currentRelease: () =>
      actual.resolveRelease({
        tag: 'v0.1.3',
        publishedAt: '2026-10-15T19:22:32Z',
        prerelease: false,
        assets: [
          { name: 'RayBend_0.1.3_x64_en-US.msi', size: 120_549_376, url: MSI_URL },
          {
            name: 'RayBend_0.1.3_x64_en-US.msi.sig',
            size: 416,
            url: `${MSI_URL}.sig`,
          },
        ],
      }),
  };
});

import { Hero } from './Hero.tsx';

describe('Hero 主按钮的去处', () => {
  it('有正式版时指向安装包直链，而不是 GitHub 发布页', () => {
    const { container } = render(() => <Hero />);
    const hrefs = [...container.querySelectorAll('a[href]')].map((a) =>
      a.getAttribute('href'),
    );
    expect(hrefs).toContain(MSI_URL);
    expect(hrefs.some((href) => href?.includes('/releases/tag/'))).toBe(false);
  });

  it('说明文字随发布状态切换（有正式版时报版本号，不再说「关注发布页」）', () => {
    const { container } = render(() => <Hero />);
    expect(container.textContent).toContain('v0.1.3');
  });
});
