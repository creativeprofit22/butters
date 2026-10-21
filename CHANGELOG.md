# Changelog

What changed in each Butters release. Newest first. Before releasing, rename
`Unreleased` to the new version (`## [X.Y.Z] - YYYY-MM-DD`): `scripts/release.sh`
refuses to run without that section, and the release workflow uses it as the
GitHub release notes.

## Unreleased

- Docker clean-ups no longer delete data that can't be recovered: containers started with `--rm` are flagged, and locally built images are skipped.
- Live meters run off the main thread, Docker scans no longer overlap, and the old-image check is cached.
- A corrupt data file is kept aside instead of being lost; failed saves and shortcut errors are logged and reported.
- Docker clean-ups were sometimes recorded as Ollama models in close history.
- Local log file and crash logging, so release-build errors can be found.

## [0.1.13] - 2026-10-21

- Rotating activity log of scans, closes, alerts, updates and crashes.
- Security: rustls-webpki updated to 0.103.16 (RUSTSEC-2026-0382).

## [0.1.12] - 2026-10-20

- Docker is closed last, because its clean-ups and containers need the engine running.

## [0.1.11] - 2026-10-20

- Windows: detection handles backslash paths, out-of-order process ids, private-bytes memory and `.exe` names; CLI tools aren't treated as apps.
- Leaks are flagged while still small.

## [0.1.10] - 2026-10-20

- Detection measures real memory use (idle tools were under-counted by about 99%).
- Recognises Python, Flutter, Android, Gradle, `go run` and GG Coder; shows MCP server names and runtime labels.
- New README; Windows marked beta.

## [0.1.9] - 2026-10-19

- Fixed a magenta line on list rows.
- Closes run in parallel with instant feedback.
- Butters Playground test app (build from source).

## [0.1.8] - 2026-10-19

- The panel stays open while apps close; smoother batch melt.

## [0.1.7] - 2026-10-19

- Tooltip cards; Docker results ready instantly from the background check.
- Removed port lookup.

## [0.1.6] - 2026-10-19

- Docker: old image versions and full build-cache cleanup with last-used age.

## [0.1.5] - 2026-10-19

- Docker deep dive, leak watch, fan culprits, end-of-day Sweep and a global shortcut (⌥⌘B).

## [0.1.4] - 2026-10-19

- Lifetime stats screen.
- Notifications; clicking one opens the panel.

## [0.1.3] - 2026-10-19

- CPU gauge is now a sizzling pan of butter.

## [0.1.2] - 2026-10-19

- Butter stick memory gauge, melt animations, smoothness score and toast mode.

## [0.1.1] - 2026-10-19

- Melting panel exit, animated status pat and tray alert.
- Notarized DMGs.

## [0.1.0] - 2026-10-19

- First release.
