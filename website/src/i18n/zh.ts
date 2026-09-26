/**
 * 中文文案 —— **本站文案的唯一来源**（`en.ts` 必须与它结构完全一致，由类型强制）。
 *
 * 写作约定：
 * - 讲**能力**，不讲内部术语（`flowbar`、`catalog` 这类词不出现在官网上）；
 * - 不承诺日期；未实现的模块用「规划中」的口径描述，具体状态由页面统一声明；
 * - 数字与英文技术词保持与仓库其他文档一致（AGPL-3.0、RAW、XMP…）。
 */
export const zh = {
  meta: {
    title: '光伴 RayBend — 开源相片管理软件｜导入 · 浏览 · 整理 · 导出',
    description:
      '光伴（RayBend）是一款本地优先的开源相片管理软件：导入、浏览、评级、筛选、整理、导出做成一条顺手的流水线。照片只留在你自己的硬盘上。免费开源（AGPL-3.0）。',
  },

  nav: {
    brand: '光伴',
    features: '功能',
    download: '下载',
    tutorials: '教程',
    github: 'GitHub',
    downloadCta: '下载',
    language: '语言',
  },

  hero: {
    earlyBadge: '早期开发中',
    tagline: '本地优先的开源相片管理软件',
    intro:
      '把「导入 → 浏览 → 评级 → 筛选 → 整理 → 导出」做成一条顺手的流水线。照片、评分、标签都留在你自己的硬盘上 —— 没有账号，不用订阅，也不上传任何东西。',
    ctaPrimary: '下载 Windows 版',
    ctaSecondary: '在 GitHub 看源码',
    facts: ['Windows 10/11', '免费开源 AGPL-3.0', '照片只存本地'],
    earlyNote:
      '目前处于早期开发。想第一时间拿到可用版本，关注发布页即可。',
    shotCaption: '应用截图（浏览工作区）· 待补',
  },

  highlights: {
    title: '为什么值得关注',
    items: {
      local: {
        title: '照片不出门',
        body: '照片与数据库全部在本地，不经过任何云服务。你的库就是一个普通的文件夹，随时可以用文件管理器打开。',
      },
      fast: {
        title: '快得起来',
        body: '缩略图优先读取相机内嵌的 JPEG 预览 —— 比完整解码快一到两个数量级，网格滚动始终跟手。',
      },
      open: {
        title: '开源且许可清楚',
        body: 'AGPL-3.0 授权，代码公开、可审计、可自建。工具是你的，不是租来的。',
      },
    },
  },

  workflows: {
    title: '一条完整的工作流',
    subtitle: '从相机卡到成品照片，四个阶段各管一段 —— 也是这个软件想为你省下的那部分力气。',
    stageLabel: '阶段',
    items: {
      import: {
        title: '导入',
        body: '从相机卡或硬盘批量入库：导入模版与自动序号、RAW 与 JPEG 分流、重复检测、可暂停可续传。',
      },
      browse: {
        title: '浏览',
        body: '三列工作区：左边选库与目录，中间网格看片，右边看参数。按时间分组，用键盘一张张过。',
      },
      organize: {
        title: '整理与检索',
        body: '层级标签、集合与智能集合、支持中文的全文搜索、地图与时间线、重复与相似自动分组。',
      },
      export: {
        title: '导出',
        body: '尺寸与格式预设、批量导出队列、元数据保留策略与 XMP 互操作 —— RAW 原文件永不改动。',
      },
    },
  },

  features: {
    title: '功能细节',
    subtitle: '几件想清楚了再动手的事。',
    items: {
      import: {
        title: '建一个说得清的库',
        body: '导入不是「把文件拖进去」，而是让库从一开始就有秩序。',
        bullets: [
          '导入模版：用变量拼出 2026-08-15/MYP0001.png 这样的结构，序号自动递增、永不撞名。',
          'RAW 与 JPEG 自动分流：同名的 RAW 进同级的 _RAW 目录，列表里一眼干净。',
          '目录透传：保留你原来的子目录结构，不把几百个文件拍平成一坨。',
          '先落元数据、再做解码：进度条立刻动起来，不必等整批解码完。',
        ],
      },
      browse: {
        title: '看片、挑片、打标',
        body: '整个评片过程不用在窗口之间来回切。',
        bullets: [
          '三列同框：库与目录、网格、参数与直方图并排。',
          '按时间分组：同一天的照片聚在一起，相邻超过一小时自动断片。',
          '三态标记：评分、色标、喜欢、旗标、锁；多选时自动显示「混合态」，批量改一步到位。',
          '筛选即视图：把标记控件一键切成筛选条件，直接从当前视野里挑出同类照片。',
        ],
      },
      keyboard: {
        title: '手不用离开键盘',
        body: '评片是最需要肌肉记忆的环节，所以键盘优先。',
        bullets: [
          '全键盘完成「看片 → 打分 → 下一张」，不需要碰鼠标。',
          '命令面板：模糊搜索所有操作，顺带显示快捷键与最近使用。',
          '快捷键可自定义、可导入导出，冲突会当场提示。',
        ],
      },
    },
  },

  oss: {
    title: '开源，也把底子摊开讲',
    body: '没有黑箱：用什么做的、怎么处理照片、许可怎么算，都写在仓库里。',
    items: {
      license: {
        title: 'AGPL-3.0',
        body: '自由使用、可自建、可审计。若对外提供网络服务，需按同许可开放对应源码。',
      },
      stack: {
        title: 'Rust + Tauri + wgpu',
        body: '原生渲染，而不是把照片塞进浏览器。图像处理与色彩管线都在 Rust 侧。',
      },
      privacy: {
        title: '无账号、无遥测',
        body: '不注册、不统计、不上传。断网也能正常整理你的照片库。',
      },
    },
    cta: '去 GitHub 看看',
  },

  download: {
    title: '下载',
    subtitle: 'Windows 桌面版，安装包免费、无需注册。',
    versionLabel: '版本',
    versionPending: '即将发布',
    releaseDateLabel: '发布日期',
    sizeLabel: '安装包大小',
    platformLabel: '平台',
    platform: 'Windows 10 / 11（64 位）',
    licenseLabel: '许可',
    license: 'AGPL-3.0-only（免费）',
    ctaRelease: '前往 GitHub 发布页',
    ctaDownload: '下载安装包',
    pendingNote:
      '还没有发布安装包 —— 第一个版本发布后，这里的版本号与下载按钮会自动更新到最新正式版。',
    readyNote: '点上面的按钮即可下载最新正式版；历史版本与校验值见发布页。',
    requirementsTitle: '系统要求',
    requirements: ['Windows 10 / 11（64 位）', '内存 4 GB 以上', '磁盘空间按你的照片库大小准备'],
    smartscreenNote:
      '请核对发布页的签名状态与 SHA-256。未签名或信誉不足时 Windows 可能提醒或阻止运行；Smart App Control 与企业策略下未必有继续运行选项。',
    sourceNote: '也可以',
    sourceLink: '从源码构建',
    sourceSuffix: '（需要 Rust 与 pnpm）。',
  },

  tutorials: {
    title: '视频教程',
    subtitle: '从导入到整理，边看边做。',
    comingSoonTitle: '教程录制中',
    comingSoonBody: '视频会发布在 B 站，上线后直接搬到这里。',
  },

  footer: {
    tagline: '本地优先的开源相片管理软件。',
    product: '产品',
    resources: '资源',
    source: '源码',
    releases: '发布页',
    issues: '问题反馈',
    license: '许可',
    changelog: '变更记录',
    builtWith: '本站用 Solid 构建，与应用本体是两套技术栈。',
    rights: '© 2026 RayBend · AGPL-3.0-only',
  },

  notFound: {
    title: '页面不存在',
    body: '链接可能过期了，或者地址写错了。',
    back: '回到首页',
  },
};

/** `en.ts` 必须满足同一份结构；`zh` 是文案形状的唯一事实来源 */
export type Dictionary = typeof zh;
