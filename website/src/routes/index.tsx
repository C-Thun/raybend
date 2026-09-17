import { Hero } from '../sections/Hero.tsx';
import { Highlights } from '../sections/Highlights.tsx';
import { Workflows } from '../sections/Workflows.tsx';
import { Features } from '../sections/Features.tsx';
import { OpenSource } from '../sections/OpenSource.tsx';
import { DownloadSection } from '../sections/DownloadSection.tsx';
import { Tutorials } from '../sections/Tutorials.tsx';

/**
 * 首页 = 一串区块，顺序即叙事：
 * hero（琥珀贴顶）→ 三个卖点 → 四个阶段 → 三条功能细节 → 开源与技术 → 下载 → 教程。
 */
export default function Home() {
  return (
    <>
      <Hero />
      <Highlights />
      <Workflows />
      <Features />
      <OpenSource />
      <DownloadSection />
      <Tutorials />
    </>
  );
}
