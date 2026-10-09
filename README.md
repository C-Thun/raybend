<div align="center">

<img src="assets/logo-wide.webp" alt="RayBend · 光伴" width="440" />

**Local-first, open-source photo management**

Import → Browse → Edit → Export, one complete workflow. Free, no subscription, nothing leaves your disk.

English ｜ [简体中文](README.zh-CN.md) ｜ [Website](https://raybend.cthun.com/) ｜ [Download](https://github.com/C-Thun/raybend/releases)

</div>

---

Commercial photo managers are expensive or subscription-based, and a photo library is built over decades — the software that keeps it in order should not be a recurring bill. RayBend is the free, open-source alternative: one workflow from **Import → Browse → Edit → Export**, with photos, edits and organization all on your own disk. No account, no upload.

## Features

- **One workflow** — Import, Browse, Edit and Export: four full workspaces in sequence, a single switch apart.
- **The interface is the feature set** — every action sits on the screen as a button rather than buried in menu hierarchies; commands and keyboard shortcuts remain for practiced hands.
- **Multi-source import** — draw from several sources at once, multi-slot cameras included; names and folder structure follow templates, with serial numbers that increment automatically.
- **Grouped by time** — photos fall into days, and into sessions within the day.
- **Multi-image compare** — candidates side by side on one screen.
- **Non-destructive editing** — colour grading at the core; no adjustment ever touches the original, and one photo can hold several saved looks.
- **Open file formats** — ratings, labels and edits travel in standard XMP sidecars that other software understands; the original RAW is never rewritten.
- **Everything local** — no account, no upload, fully usable offline.
- **Appearance & language** — light and dark themes, two density settings, English and Chinese — all a toggle away.

## Supported formats

| | Formats |
| --- | --- |
| **Photos** | JPG · PNG · TIFF · WebP · AVIF · HEIC \* |
| **Export** | JPG · PNG · WebP · AVIF |
| **Camera RAW** | `ari` `arw` `cr2` `cr3` `crw` `dcr` `dcs` `dng` `erf` `iiq` `kdc` `mef` `mos` `mrw` `nef` `nrw` `orf` `pef` `raf` `rw2` `sr2` `srf` `srw` `3fr` |

\* HEIC files are imported and indexed; previews need a decoder that is not bundled yet.

RAW import and decoding follow [rawler](https://github.com/dnglab/dnglab), so camera coverage tracks upstream releases. A JPG and a RAW file sharing one name are treated as two files of a single photo.

## Download

For Windows 10 / 11 (64-bit), free of charge. Installers on [GitHub Releases](https://github.com/C-Thun/raybend/releases).

## Build from source

Requires Node with pnpm, and the pinned Rust toolchain (`rust-toolchain.toml`). Day-to-day development runs in WSL/Linux; producing the Windows product additionally needs Windows-side Rust/MSVC and a one-off dav1d static library — the full story is in [docs/release.md](docs/release.md).

```bash
pnpm install
pnpm tauri dev        # desktop window
pnpm build            # frontend build; dist/ is embedded into the executable at compile time
```

The complete command index — tests, quality gates, Windows debug builds, AI assets — is in [docs/development.md](docs/development.md).

## Tech stack

Rust · Tauri 2 · wgpu (native GPU rendering) · SolidJS · Tailwind CSS · SQLite · rawler (RAW decoding)

## Documentation

- User guide (also available with F1 in the app): [docs/user-guide.md](docs/user-guide.md) · [English](docs/user-guide.en.md)
- Development commands: [docs/development.md](docs/development.md) (Chinese)
- Release, signing and update channels: [docs/release.md](docs/release.md) (Chinese)
- Privacy: [docs/privacy.md](docs/privacy.md) · [English](docs/privacy.en.md)
- Engineering decisions and memory: [AGENTS.md](AGENTS.md), [memory/](memory/) (Chinese)

## License

[AGPL-3.0-only](LICENSE); third-party notices in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md). Should you ever offer RayBend as a network service, AGPL §13 requires releasing the corresponding source under the same license.
