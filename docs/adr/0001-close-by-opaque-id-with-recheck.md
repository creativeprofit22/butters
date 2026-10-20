---
status: accepted
---

# Close by opaque id, re-checked at close time

## Context and Problem Statement

The panel shows suggestions from a scan that may be seconds or minutes old. By the time the user clicks Close, a pid can have exited and been reused by something else, or the target may have become the app in front. Closing the wrong process is the worst thing Butters can do.

## Considered Options

- Frontend sends pids; backend closes them.
- Frontend sends opaque ids from the last scan; backend re-verifies each target before closing.

## Decision Outcome

Chosen: opaque ids with a re-check. The backend maps the id to its own record and refuses unless the pid still has the same start time, belongs to the current user, and is not pid 0/1, Butters, its launcher or the frontmost app. Closing is graceful first; force close is a separate request the user confirms.

## Consequences

- Good: pid reuse and stale scans can't close the wrong process; the frontend can't be used to end arbitrary pids.
- Bad: every command that closes something must go through the id table; ids expire with each scan.
