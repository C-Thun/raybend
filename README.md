<div align="center">

<img src="assets/logo-wide.webp" alt="RayBend · 光伴" width="440" />

**光伴 · Local-first, open-source photo management**

Import → Browse → Edit → Export, one complete workflow. Free, no subscription, nothing leaves your disk.

English ｜ 简体中文 ｜ [Website](https://raybend.cthun.com/) ｜ [Download](https://github.com/C-Thun/raybend/releases)

</div>

---

## Why RayBend

Photo managers today come in three flavors: too expensive, subscription-based, or reasonably priced but unstable. A photo library is built over decades; the tool that keeps it in order should not be a recurring bill.

RayBend takes a different position: one complete workflow — Import → Browse → Edit → Export — where your photos and every change you make stay on your own disk. No account, no upload. Free, forever — open source under AGPL-3.0.

## Features

- **One workflow** — Import, Browse, Edit, Export: four full workspaces in sequence, a single switch apart.
- **The interface is the feature set** — Every action sits on the screen as a button, not buried in menu hierarchies; commands and keyboard shortcuts remain, as advanced options for practiced hands.
- **Multi-source import** — A single import can draw from several sources at once, multi-slot cameras included; names and folder structure follow templates, with serial numbers that increment automatically.
- **Grouped by time** — Photos fall naturally into days and sessions within the day; take in the shape of a day first, a single frame second.
- **Multi-image compare** — Candidates side by side on one screen; choose with the full picture in view.
- **Non-destructive editing** — Color grading at the core; no adjustment ever touches the original. One photo, many versions; styles saved and reusable.
- **Everything local** — No account, no upload, fully usable offline; your photos and the structure you give them stay on your own disk.
- **Appearance & language** — Light and dark themes, two density settings, English and Chinese — all a toggle away.

## Download

For Windows 10 / 11 (64-bit), free of charge. Installers on [GitHub Releases](https://github.com/C-Thun/raybend/releases).

## Build from source

```bash
pnpm install
pnpm tauri dev
```

Requires pnpm and the Rust toolchain.

## Tech stack

Rust · Tauri 2 · wgpu (native GPU rendering) · SolidJS · Tailwind CSS · SQLite · rawler (RAW decoding)

## License

[AGPL-3.0-only](LICENSE); third-party notices in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md). Should you ever offer RayBend as a network service, AGPL §13 requires releasing the corresponding source under the same license.
