# Glossary

**Process** — one OS process, reduced to what the rules need (memory, CPU, idle time, ports, parent).

**Group** — a set of processes the user thinks of as one thing: "Chrome", "the Vite server for my-app", "that Claude Code session". The unit Butters suggests and closes.
Avoid: item, entry (in code).

**Kind** — what a group is (agent session, MCP server, dev server, watcher, language server, local model, Docker container, Ollama model, desktop app...). Decided from argv[0] only.

**Local model** — a kind for heavy local runtimes started as a process: model servers (llama.cpp, MLX, vLLM, LM Studio) and the Android emulator.
Avoid: confusing with *Ollama model*.

**Ollama model** — a model currently loaded inside Ollama, closed by unloading it through Ollama, not by ending a process.

**Orphaned** — a process whose launcher (terminal, session) is gone while it keeps running. A technical condition, never shown to users.
Avoid: zombie, stray.

**Leftover** — an orphaned group of a dev kind (agent session, MCP server, dev server, watcher, language server) that isn't a daemon by design. The user-facing word; always recommended.
Avoid: orphan (in UI copy).

**Idle** — seconds since a process last did noticeable work (CPU or terminal I/O). A *lower bound* when Butters only started watching it recently.

**Suggestion** — a group (or container, model, Docker disk) the panel offers to close, with a reason. *Recommended* suggestions are pre-ticked.
Avoid: candidate (outside internal filtering).

**Protected** — a group Butters refuses to suggest: the frontmost app, anything used in the last 15 minutes, work inside the app in front, Butters and its launcher, system and other users' processes.

**Ignored** — a suggestion the user asked never to see again, remembered by its stable key.
Avoid: hidden, dismissed.

**Close** — end a group the polite way first (app Quit, graceful signal). **Force close** is a separate, confirmed request when that fails.
Avoid: kill, terminate (in UI copy).

**Sweep** — one end-of-day action that closes leftovers, idle agents, idle dev servers, MCP servers, watchers, long-idle containers and Ollama models, after confirmation.

**Freed** — memory released by closing, counted per day and for lifetime stats.

**Smoothness score** — 0–100 for a day: smooth minutes count fully, busy half, strained not at all. Shown as "Buttery N".

**Level** — the machine's current state: smooth, busy or strained.
