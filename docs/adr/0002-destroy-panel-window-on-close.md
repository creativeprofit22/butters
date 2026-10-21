---
status: accepted
---

# Destroy the panel window on close instead of hiding it

## Context and Problem Statement

Butters sits in the menu bar all day and promises to barely use anything itself. A hidden webview window keeps its memory.

## Considered Options

- Keep one panel window and hide/show it.
- Create the window on open and destroy it on close.

## Decision Outcome

Chosen: destroy on close. Idle Butters stays around 20 MB because the webview only exists while the panel is open.

## Consequences

- Good: low idle memory; every opening starts from a clean state.
- Bad: timing matters. The close delay must outlast the melt animation, blur needs a grace period after open, and the frontend must hold the panel open while closing apps steals focus. All state that must survive lives in Rust.
