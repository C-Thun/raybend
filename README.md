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
| `pnpm build` | Frontend build into `dist/` (**run it before any Windows build** — `dist/` is embedded into the executable) |
| `pnpm tauri dev` | Desktop window |
| `pnpm tauri <args>` | Tauri CLI passthrough; AI assets resolve per `--ai=auto\|required\|off` |
| `pnpm preview` | Serve the built frontend |

### Windows build and debug

The Windows side is where the product is verified; `pnpm build` must run first (see above).

| Command | What it does |
| --- | --- |
| `pnpm debug:win` | One-shot debug build: frontend → Windows cargo → artifact check (accepts `-- --ai=…`) |
| `pnpm check:win` | Artifact check: timestamps against `dist/`, embedded assets, worker protocol |
| `pnpm clean:win` / `pnpm clean:wsl` | Drop stale target artifacts without paying for a cold rebuild |
| `pnpm check:color-win [profile…]` | Windows-side colour probes (builds and runs the colour examples on the Windows target) |
| `pnpm check:lens-ipc-win --launch <repository-id> <asset-id>` | Read-only lens-metadata IPC smoke; `--expect-profile=` / `--expect-metadata-warning` assert details |

### Release (run by a human)

| Command | What it does |
| --- | --- |
| `pnpm release <test\|patch\|minor\|major> [--channel beta\|test\|release] [--win-msi\|--win-nsis] [--unsigned] [--with-updater] [--dry-run] [--allow-dirty] [--skip-build] [--win-dir <dir>]` | Bump, build, package and produce `release-out/` — no commit, tag or upload |
| `pnpm release:finalize <bundle> --manifest <json> --out <dir> [--allow-unsigned] [--base-url <https://…>]` | Manual path: hashes, manifests and update JSON for an existing bundle |
| `pnpm release:publish <release-out/vX.Y.Z> [--execute] [--no-crates]` | Read-only preview by default; `--execute` commits, tags, pushes, creates the Release, uploads assets, publishes the core crate to crates.io and waits for the website workflow. `--no-crates` skips crates.io; beta / test releases never publish it |
| `pnpm licenses:generate` | Regenerate `public/legal/third-party.json` |

### AI assets (offline photo tagging)

Recognition runs locally: the model pack and the CPU runtime are **build inputs**, never downloads at runtime.
Build commands (`pnpm tauri`, `pnpm debug:win`, `pnpm release`) all take `--ai=auto|required|off` — `auto` (default) uses a registered pack, or falls back to the no-AI variant (printing the reason) when there is none; `required` fails unless every input is present and verified; `off` builds without touching the source or the network.

| Command | What it does |
| --- | --- |
| `pnpm ai:prepare [--ai=auto\|required\|off]` | Resolve and freeze this build's AI inputs (model pack + CPU runtime DLLs); prints the plan as JSON |
| `pnpm ai:use [-- <library>]` | Register an **already exported** pack (a fresh clone plus `git lfs pull` is enough) |
| `pnpm ai:library:init [-- <path>]` | Initialise a model library at `<path>`, or the per-user default when omitted |
| `pnpm ai:export [-- <library>]` | Export/verify the pack through the library and register it for this project |

The model itself lives in the separate **model-registry** repository, cloned next to this checkout. RayBend's git keeps only the compatibility contract and the review digests; the local pointer (`ai-model-source.local.json`) is git-ignored.

```bash
# one-time: get the library next to the raybend checkout
git clone https://github.com/C-Thun/model-registry ../model-registry
cd ../model-registry && git lfs install && git lfs pull   # the ONNX encoder is LFS-tracked: pointers are not weights

# register the pack and build
cd ../raybend
pnpm ai:use -- ../model-registry     # packs already exported and committed: just register
pnpm debug:win -- --ai=required      # --ai=off gives the no-AI variant
```

Building the pack from scratch (only when the model or recipe changes) needs Python 3.12, network access, ≥4 GiB free space and Git LFS:

```bash
cd ../model-registry
pnpm model:export                    # pinned upstream revision → FP32 / opset17 ONNX → library commit

cd ../raybend
pnpm ai:library:init -- ../model-registry   # only if the library has no marker yet
pnpm ai:export -- ../model-registry         # export + RayBend profile + local source registration
pnpm ai:prepare -- --ai=required            # verify and freeze the inputs before building
```

Notes: CPU only — no CUDA, Python or PyTorch dependency in the product; `RAYBEND_AI_RUNTIME_DIR` can point at a local onnxruntime directory instead of the verified cache / pinned official ZIP; re-exporting to different digests is a review event, not an automatic update (protocol: [specs/ai-model-library-build.md](specs/ai-model-library-build.md), v1 thresholds and quality: [docs/ai/tinyclip-v1/README.md](docs/ai/tinyclip-v1/README.md)).

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

### Performance

| Command | What it does |
| --- | --- |
| `pnpm perf:browse [rows]` | Grid virtualisation benchmark (default 100 000 rows) |
| `pnpm perf:grid [rows]` | Grid pipeline throughput: time grouping, row model, virtual window |
| `pnpm perf:win [--launch]` | Real-machine browse sampling over CDP on Windows |

### Occasional and historical tools

Nothing here is part of the build or release path.

| Command | What it does |
| --- | --- |
| `pnpm shot` | Screenshots by theme / density / URL / size — dev-time visual self-check |
| `pnpm crash:drill` / `pnpm migrate:drill` | Storage drills from the M1 work (worker-crash isolation; catalog migration / backup / corruption) — rerun when touching recovery |
| `pnpm spike:win` | The M0-2 / M2-W1 rendering spike (build → open window → measure → write report); kept as a measuring tool |
| `scripts/build-dav1d-win.cmd` | One-off Windows helper that builds the dav1d static library the Windows link step needs (its environment variables are wired in `scripts/lib/dav1d-win.mjs`) |
| `scripts/ai/*.py`, `scripts/ai/*.ps1` | Research and archive scripts for the model work (calibration replays, reference vectors, preprocessing parity, Windows worker probes) — no product dependency |

Internal modules live in `scripts/lib/` (`release-*.mjs`, `ai-*.mjs`, `cdp.mjs`, …): libraries for the entry points above, not commands by themselves.

## Tech stack

Rust · Tauri 2 · wgpu (native GPU rendering) · SolidJS · Tailwind CSS · SQLite · rawler (RAW decoding)

## Documentation

- User guide (also available with F1 in the app): [docs/user-guide.md](docs/user-guide.md) · [English](docs/user-guide.en.md)
- Release, signing and update channels: [docs/release.md](docs/release.md)
- Privacy: [docs/privacy.md](docs/privacy.md) · [English](docs/privacy.en.md)
- Engineering decisions and memory (Chinese): [AGENTS.md](AGENTS.md), [memory/](memory/)

## License

[AGPL-3.0-only](LICENSE); third-party notices in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md). Should you ever offer RayBend as a network service, AGPL §13 requires releasing the corresponding source under the same license.
