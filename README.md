<div align="center">

<img src="assets/logo-wide.webp" alt="RayBend · 光伴" width="440" />

**光伴 · Local-first, open-source photo management**

Import → Browse → Edit → Export, one complete workflow. Free, no subscription, nothing leaves your disk.

English ｜ [简体中文](README.zh-CN.md) ｜ [Website](https://raybend.cthun.com/) ｜ [Download](https://github.com/C-Thun/raybend/releases)

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
- **Non-destructive editing** — Color grading at the core; no adjustment ever touches the original. One photo, many versions; looks saved and reusable.
- **Open file formats** — Ratings, labels and edits travel in standard XMP sidecars that other software understands. The original RAW is never rewritten.
- **Everything local** — No account, no upload, fully usable offline; your photos and the structure you give them stay on your own disk.
- **Appearance & language** — Light and dark themes, two density settings, English and Chinese — all a toggle away.

## Supported formats

### Photos (bitmap)

| Direction | Formats |
| --- | --- |
| **Import** | JPG · PNG · TIFF · WebP · AVIF · HEIC \* |
| **Export** | JPG · PNG · WebP · AVIF |

\* HEIC files are recognised, imported and indexed; decoding still needs a libheif-based decoder, so HEIC tiles currently show a placeholder.

### Camera RAW

RAW import follows [rawler](https://github.com/dnglab/dnglab). The extensions the scanner recognises today:

`ari` `arw` `cr2` `cr3` `crw` `dcr` `dcs` `dng` `erf` `iiq` `kdc` `mef` `mos` `mrw` `nef` `nrw` `orf` `pef` `raf` `rw2` `sr2` `srf` `srw` `3fr`

Two deliberately independent paths:

- **Thumbnails (grid, filmstrip)** come from the JPEG the camera embeds inside the RAW file, without touching sensor data — standard TIFF `JPEGInterchangeFormat` entries, Panasonic `JpgFromRaw`, JPEG-compressed preview strips in DNG/3FR, and Olympus / OM System MakerNote previews. It is a millisecond-scale path, and it works even for camera bodies the decoder database does not know yet.
- **Full decoding (view, edit)** goes through rawler inside an isolated worker process, so a bad file cannot take the application down. Bayer sensors are broadly covered; Fujifilm X-Trans follows rawler.

Known gaps are tracked in [`memory/FUTURE.md`](memory/FUTURE.md) §B: Sigma Foveon X3 is out of scope by decision; Nikon HE/HE★ compression, Sony's newer `arw6` compression and the newest Hasselblad bodies await upstream support — such files still show a real thumbnail, but cannot be opened in the editor yet. Olympus high-resolution `.ORI` and Hasselblad `.FFF` are not wired into the scanner yet.

A JPG and a RAW file sharing one name are treated as two files of a single photo.

## Download

For Windows 10 / 11 (64-bit), free of charge. Installers on [GitHub Releases](https://github.com/C-Thun/raybend/releases).

## Build from source

Requirements: Node with pnpm, and the pinned Rust toolchain (`rust-toolchain.toml`). Day-to-day development runs in WSL/Linux; producing the Windows product additionally needs Windows-side Rust/MSVC and a one-off dav1d static library — the full story is in [docs/release.md](docs/release.md).

```bash
pnpm install
pnpm tauri dev        # desktop window
pnpm build            # frontend only; dist/ is embedded into the executable at compile time
```

## Command line

Everything is a pnpm script (plus cargo). The complete release procedure lives in [docs/release.md](docs/release.md); this is the index.

### Develop

| Command | What it does |
| --- | --- |
| `pnpm dev` | Vite dev server only — the page the browser checks below talk to |
| `pnpm tauri dev` | Desktop window |
| `pnpm build` | Frontend build into `dist/` (**run it before any Windows build** — `dist/` is embedded into the executable) |
| `pnpm preview` | Serve the built frontend |
| `pnpm tauri <args>` | Tauri CLI passthrough; AI assets resolve per `--ai=auto\|required\|off` |

### Quality gates

| Command | What it does |
| --- | --- |
| `pnpm typecheck` | `tsc --noEmit` |
| `pnpm test` | Frontend unit tests |
| `pnpm test:release` | Unit tests for the release scripts |
| `pnpm lint:colors` | No hard-coded colours outside the token layer |
| `pnpm lint:arch` | Frontend layering and dependency direction |
| `pnpm lint:i18n` | Translation coverage |
| `cargo test --workspace` | Rust tests (imaging, RAW, store, migrations) |

### Smoke and regressions

The browser checks need `pnpm dev` in another terminal.

| Command | What it does |
| --- | --- |
| `pnpm smoke:ui [url]` | The page really renders — a console-error gate against white screens |
| `pnpm check:startup` | Startup / system preferences |
| `pnpm check:flowbar` | Workflow bar switching |
| `pnpm check:browse` | Entering Browse with a non-empty library |
| `pnpm check:import` | Import confirmation flow up to `import_start` |
| `pnpm check:export` | Export workspace boot |
| `pnpm check:external` | External editor round-trip |
| `pnpm check:color-status [url]` | Colour pipeline status probe (display profile and transform state) |
| `pnpm shot` | Screenshots by theme / density / URL / size |
| `pnpm crash:drill` | Worker-crash isolation drill |
| `pnpm migrate:drill` | Catalog migration drill |

### Performance

| Command | What it does |
| --- | --- |
| `pnpm perf:browse [rows]` | Grid virtualisation benchmark (default 100 000 rows) |
| `pnpm perf:grid [rows]` | Grid pipeline throughput: time grouping, row model, virtual window |
| `pnpm perf:win [--launch]` | Real-machine browse sampling over CDP on Windows |

### Windows build and diagnosis

| Command | What it does |
| --- | --- |
| `pnpm debug:win` | One-shot debug build: frontend → Windows cargo → artifact check |
| `pnpm check:win` | Artifact check: timestamps against `dist/`, embedded assets, worker protocol |
| `pnpm spike:win` | Rendering spike: build → open window → measure → write report |
| `pnpm clean:win` / `pnpm clean:wsl` | Drop stale target artifacts without paying for a cold rebuild |
| `pnpm check:color-win [profile…]` | Windows-side colour probes (builds and runs the colour examples on the Windows target) |
| `pnpm check:lens-ipc-win --launch <repository-id> <asset-id>` | Read-only lens-metadata IPC smoke; `--expect-profile=` / `--expect-metadata-warning` assert details |

### Release (run by a human)

| Command | What it does |
| --- | --- |
| `pnpm release <test\|patch\|minor\|major> [--channel beta\|test\|release] [--win-msi\|--win-nsis] [--unsigned] [--with-updater] [--dry-run] [--allow-dirty] [--skip-build] [--win-dir <dir>]` | Bump, build, package and produce `release-out/` — no commit, tag or upload |
| `pnpm release:finalize <bundle> --manifest <json> --out <dir> [--allow-unsigned] [--base-url <https://…>]` | Manual path: hashes, manifests and update JSON for an existing bundle |
| `pnpm release:publish <release-out/vX.Y.Z> [--execute]` | Read-only preview by default; `--execute` commits, tags, pushes, creates the Release, uploads assets and waits for the website workflow |
| `pnpm licenses:generate` | Regenerate `public/legal/third-party.json` |

### AI assets

| Command | What it does |
| --- | --- |
| `pnpm ai:prepare [--ai=auto\|required\|off]` | Validate or materialise the local AI runtime and model assets for a build |
| `pnpm ai:library:init` / `ai:export` / `ai:use` | Manage the local model library and the model source the build points at |

## Tech stack

Rust · Tauri 2 · wgpu (native GPU rendering) · SolidJS · Tailwind CSS · SQLite · rawler (RAW decoding)

## Documentation

- User guide (also available with F1 in the app): [docs/user-guide.md](docs/user-guide.md) · [English](docs/user-guide.en.md)
- Release, signing and update channels: [docs/release.md](docs/release.md)
- Privacy: [docs/privacy.md](docs/privacy.md) · [English](docs/privacy.en.md)
- Engineering decisions and memory (Chinese): [AGENTS.md](AGENTS.md), [memory/](memory/)

## License

[AGPL-3.0-only](LICENSE); third-party notices in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md). Should you ever offer RayBend as a network service, AGPL §13 requires releasing the corresponding source under the same license.
