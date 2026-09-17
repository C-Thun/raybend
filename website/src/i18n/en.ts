import type { Dictionary } from './zh.ts';

/**
 * English copy. Must mirror `zh.ts` exactly — the `Dictionary` type enforces it
 * at compile time, so a missing key is a type error rather than a blank spot on
 * the page. Natural English, not a literal translation.
 */
export const en: Dictionary = {
  meta: {
    title: 'RayBend — Open-source photo management for Windows',
    description:
      'RayBend is a local-first, open-source photo manager: import, browse, rate, cull, organize and export in one flow. Your photos stay on your own drive. Free and open source (AGPL-3.0).',
  },

  nav: {
    brand: 'RayBend',
    features: 'Features',
    download: 'Download',
    tutorials: 'Tutorials',
    github: 'GitHub',
    downloadCta: 'Download',
    language: 'Language',
  },

  hero: {
    earlyBadge: 'Early development',
    tagline: 'Local-first, open-source photo management',
    intro:
      'One smooth pipeline from import to browse to rate to cull to organize to export. Ratings, tags and edits live on your own drive — no account, no subscription, nothing uploaded.',
    ctaPrimary: 'Download for Windows',
    ctaSecondary: 'View on GitHub',
    facts: ['Windows 10/11', 'Free & open source (AGPL-3.0)', 'Photos stay local'],
    earlyNote:
      'Early days still. Watch the releases page to be first in line for a usable build.',
    shotCaption: 'App screenshot (browse workspace) — coming soon',
  },

  highlights: {
    title: 'Why it is worth a look',
    items: {
      local: {
        title: 'Your photos stay put',
        body: 'Photos and the catalog live on your machine — no cloud involved. A library is just an ordinary folder you can open in your file manager.',
      },
      fast: {
        title: 'Built to feel fast',
        body: 'Thumbnails come from the JPEG preview already embedded in your raw files — one to two orders of magnitude faster than a full decode, so the grid keeps up.',
      },
      open: {
        title: 'Open source, clear license',
        body: 'AGPL-3.0: the code is public, auditable and self-hostable. A tool you own, not a service you rent.',
      },
    },
  },

  workflows: {
    title: 'One complete workflow',
    subtitle:
      'From camera card to finished photo — four stages, each doing one job well. That is the work this app means to take off your hands.',
    stageLabel: 'Stage',
    items: {
      import: {
        title: 'Import',
        body: 'Pull whole cards or folders in: naming templates with automatic sequence numbers, raw/JPEG split-off, duplicate detection, pause and resume.',
      },
      browse: {
        title: 'Browse',
        body: 'Three columns: libraries and folders on the left, the grid in the middle, photo details on the right. Group by time and cull with the keyboard.',
      },
      organize: {
        title: 'Organize & search',
        body: 'Hierarchical tags, collections and smart collections, full-text search that works in Chinese too, maps and timelines, duplicate and burst grouping.',
      },
      export: {
        title: 'Export',
        body: 'Size and format presets, batch export queues, metadata retention policies and XMP interop — raw files are never modified.',
      },
    },
  },

  features: {
    title: 'The details',
    subtitle: 'A few things we got right before writing the code.',
    items: {
      import: {
        title: 'A library that stays legible',
        body: 'Importing is not "drag files in" — it is how the library gets its structure.',
        bullets: [
          'Naming templates: compose paths like 2026-08-15/MYP0001.png with variables; sequence numbers increase on their own and never collide.',
          'Raw and JPEG split apart: matching raw files go into a sibling _RAW folder, so listings stay clean.',
          'Folder passthrough: keep your existing subfolder structure instead of flattening hundreds of files into one heap.',
          'Metadata first, decoding second: rows land immediately, so the progress bar moves right away.',
        ],
      },
      browse: {
        title: 'Look, cull, tag',
        body: 'The whole culling loop happens without hopping between windows.',
        bullets: [
          'Three columns together: libraries and folders, the grid, and metadata with a histogram.',
          'Group by time: a day of shooting becomes one group; gaps over an hour start a new one.',
          'Tri-state flags: rating, color label, favorite, pick flag and lock — with multiple photos selected they show a mixed state, so batch edits are one click.',
          'Filter mode: flip the same controls into filter conditions and pull the lookalikes out of the current view.',
        ],
      },
      keyboard: {
        title: 'Hands never leave the keyboard',
        body: 'Culling is muscle memory, so the keyboard comes first.',
        bullets: [
          'Rate the photo and jump to the next one without touching the mouse.',
          'Command palette: fuzzy-search every action, with its shortcut and recent-use list right there.',
          'Shortcuts are re-bindable, importable and exportable, and conflicts are flagged on the spot.',
        ],
      },
    },
  },

  oss: {
    title: 'Open source, with the internals on the table',
    body: 'No black box: what it is built from, how photos are handled and how the license works are all in the repository.',
    items: {
      license: {
        title: 'AGPL-3.0',
        body: 'Free to use, self-host and audit. Offer it as a network service and you must publish the corresponding source under the same license.',
      },
      stack: {
        title: 'Rust + Tauri + wgpu',
        body: 'Native rendering instead of pushing photos through a browser. The image and color pipelines live in Rust.',
      },
      privacy: {
        title: 'No account, no telemetry',
        body: 'No sign-up, no analytics, no uploads. The whole library works offline.',
      },
    },
    cta: 'See it on GitHub',
  },

  download: {
    title: 'Download',
    subtitle: 'Desktop app for Windows. Free installer, no registration.',
    versionLabel: 'Version',
    versionPending: 'Coming soon',
    releaseDateLabel: 'Released',
    sizeLabel: 'Installer size',
    platformLabel: 'Platform',
    platform: 'Windows 10 / 11 (64-bit)',
    licenseLabel: 'License',
    license: 'AGPL-3.0-only (free)',
    ctaRelease: 'Go to GitHub releases',
    ctaDownload: 'Download the installer',
    pendingNote:
      'No installer has been published yet — once the first release is out, the version number and download button here update themselves to the latest stable build.',
    readyNote: 'The button above downloads the latest stable build; older builds and checksums are on the releases page.',
    requirementsTitle: 'System requirements',
    requirements: ['Windows 10 / 11 (64-bit)', '4 GB RAM or more', 'Disk space for your photo library'],
    smartscreenNote:
      'Early builds are not code-signed yet, so Windows may show a SmartScreen warning — choose "More info → Run anyway" to continue.',
    sourceNote: 'You can also',
    sourceLink: 'build from source',
    sourceSuffix: '(needs Rust and pnpm).',
  },

  tutorials: {
    title: 'Video tutorials',
    subtitle: 'Follow along from import to organizing.',
    comingSoonTitle: 'Being recorded',
    comingSoonBody: 'Videos will be published on Bilibili and embedded here once they are up.',
  },

  footer: {
    tagline: 'Local-first, open-source photo management.',
    product: 'Product',
    resources: 'Resources',
    source: 'Source code',
    releases: 'Releases',
    issues: 'Report an issue',
    license: 'License',
    changelog: 'Changelog',
    builtWith: 'This site is built with Solid — a different stack from the app itself.',
    rights: '© 2026 RayBend · AGPL-3.0-only',
  },

  notFound: {
    title: 'Page not found',
    body: 'That link may be out of date, or the address has a typo.',
    back: 'Back to the homepage',
  },
};
