# Dependency decisions

Policy: permissive licenses only (MIT, Apache-2.0, BSD, ISC, Zlib, 0BSD, Unlicense), well-maintained,
and each addition must remove risk/complexity or add clear user value. Binary size matters (~6 MB).
Stars/licenses checked with `gh api` on 2026-10-09.

## Adopted

| Library | License | Stars | Why |
|---|---|---|---|
| `tauri-plugin-single-instance` (tauri-apps/plugins-workspace) | MIT OR Apache-2.0 | 1.8k | Launching Headroom twice created two tray icons and two pollers. Second launch now just shows the existing widget. |
| `walkdir` (BurntSushi/walkdir) | Unlicense OR MIT | 1.6k | Replaces the hand-rolled recursive `jsonl_files`. Detects symlink loops (the old code could recurse forever through a looping symlink in `~/.claude/projects`) and skips unreadable entries. Already in the dependency tree via Tauri, so ~0 size cost. |

## Rejected / deferred

- **tauri-plugin-log / tracing**: only one debug `eprintln` behind `HEADROOM_DEBUG`; a logging stack adds deps and binary size for no user value. Revisit if logging grows.
- **tauri-plugin-window-state**: the widget has a fixed layout/size managed by the app; restoring state could fight the auto-sizing logic.
- **tauri-plugin-updater**: valuable but requires signing keys and release-infrastructure changes. Recommended for later, not done here.
- **notify (file watching)**: dual CC0-1.0 / Artistic-2.0 (GitHub shows no SPDX id); the analytics scan is on-demand, so a watcher adds complexity for no gain.
- **reqwest-retry / backon**: the poller already re-polls on a timer and surfaces errors; adding retry risks hammering the usage endpoint and rate limits.
- **ccusage** (18.9k stars): license reported as NOASSERTION; used only as inspiration, no code taken.

## License audit (cargo-deny 0.20, `src-tauri/deny.toml`; `cargo deny check licenses` passes)

- No GPL/AGPL/SSPL/non-commercial dependencies.
- **MPL-2.0** (file-level copyleft): `cssparser`, `cssparser-macros`, `selectors`, `dtoa-short`, `option-ext` (transitive via Tauri/dirs). Used unmodified; obligation is only to make the source of those files available, satisfied by their public crates. Allowed explicitly.
- `r-efi` is `MIT OR Apache-2.0 OR LGPL-2.1-or-later`; we use it under MIT (and it is UEFI-only, not on desktop targets).
- Attribution-style: **Unicode-3.0** (ICU crates), **CDLA-Permissive-2.0** (webpki-roots), **BSL-1.0** (ryu, also Apache-2.0), plus the MIT/Apache/BSD/ISC notice requirements: binary distributions should ship third-party notices (not yet generated).
- JS (production): only the app's own deps; all MIT/Apache. (`license-checker` shows the root package as UNLICENSED only because package.json is `private`; the repo LICENSE is MIT.)

Run: `cd src-tauri && cargo deny check licenses`.
