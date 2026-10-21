---
status: accepted
---

# Updater without default features

## Context and Problem Statement

The Tauri updater plugin's default features pull in `zip`, which brought a vulnerable crate into the build. Butters does not need zip archives to update.

## Considered Options

- Keep default features and wait for an upstream fix.
- Disable default features; ship macOS updates as the signed app bundle and Windows updates as the plain NSIS setup.exe.

## Decision Outcome

Chosen: `default-features = false`. The vulnerable crate leaves the dependency tree and both platforms still update with signed artifacts.

## Consequences

- Good: smaller, safer dependency tree.
- Bad: zip-based update formats are unavailable; re-enabling defaults (directly or through another feature) brings the crate back.
