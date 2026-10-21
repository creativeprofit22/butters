<script lang="ts">
  import { onMount } from "svelte";
  import { flip } from "svelte/animate";
  import { Tween } from "svelte/motion";
  import {
    api,
    type Notify,
    type Outcome,
    type Protected,
    type ScanResult,
    type ServicesResult,
    type Settings,
    type Smoothness,
    type Suggestion,
    type UpdateStatus,
  } from "./lib/api";
  import ButterGauges from "./lib/ButterGauges.svelte";
  import { dropFrom } from "./lib/drops";
  import { bytes } from "./lib/format";
  import Icon from "./lib/Icon.svelte";
  import { butterEase, dissolve, melt, ms, pour } from "./lib/motion";
  import ProtectedList from "./lib/ProtectedList.svelte";
  import Row from "./lib/Row.svelte";
  import Savings from "./lib/Savings.svelte";
  import SettingsView from "./lib/SettingsView.svelte";
  import StatsView from "./lib/StatsView.svelte";
  import StatusPat from "./lib/StatusPat.svelte";
  import Toast from "./lib/Toast.svelte";
  import { tip } from "./lib/tip";

  let scan = $state<ScanResult | null>(null);
  let services = $state<ServicesResult | null>(null);
  let error = $state<string | null>(null);
  let refreshing = $state(false);
  let view = $state<"main" | "settings" | "stats">("main");
  let settings = $state<Settings | null>(null);
  let showProtected = $state(false);

  // Keyed by what a suggestion *is*, so choices survive the 15 s refresh.
  let overrides = $state<Record<string, boolean>>({});
  let results = $state<Record<string, Outcome>>({});
  let pending = $state<Record<string, boolean>>({});
  let confirming = $state(false);
  let running = $state(false);
  let toast = $state("");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  // Rows play their melt, then leave the list (the rest glide up).
  let melting = $state<Record<string, boolean>>({});
  let gone = $state<Record<string, boolean>>({});
  // The big "you saved" moment after closing things.
  let savings = $state<{ freed: number; closed: number; cpu: number; ports: number } | null>(null);
  let update = $state<UpdateStatus | null>(null);
  // A protected app the user chose to close: first click arms, second confirms.
  let armedKey = $state<string | null>(null);
  // Physics melt: drops fall from melting rows and splash on the footer edge.
  let dropLayer: HTMLDivElement | undefined = $state();
  let footerEl: HTMLElement | undefined = $state();
  let smooth = $state<Smoothness | null>(null);
  let statsTab = $state<"life" | "fan">("life");
  // End-of-day sweep: the ids the backend says are safe to close together.
  let sweepIds = $state<string[]>([]);
  let notify = $state<Notify>("on");
  // Butters Playground (a separate test build): a bar to reset its test processes.
  let playground = $state<{ running: number; total: number; node: boolean } | null>(null);
  let resetting = $state(false);
  async function resetPlayground() {
    resetting = true;
    try {
      playground = await api.playgroundReset();
      await sleep(ms(600)); // let them start listening
      await refresh();
    } catch (e) {
      say(String(e));
    } finally {
      resetting = false;
    }
  }
  let updating = $state(false);
  // Rows pour in one after another only on the first fill.
  let firstFill = $state(true);
  let panelEl: HTMLElement;

  // Rows that are mid-melt stay in the list (as they were) until their melt
  // finishes, even if a refresh no longer lists them, so a row is never torn
  // out halfway and never gets a second exit animation on top.
  let meltingRows = $state<Record<string, Suggestion>>({});
  const items = $derived(
    [
      ...[...(scan?.suggestions ?? []), ...(services?.suggestions ?? [])].filter((s) => !meltingRows[s.key]),
      ...Object.values(meltingRows),
    ]
      .filter((s) => !gone[s.key])
      .sort((a, b) => Number(b.recommended) - Number(a.recommended) || b.memory - a.memory),
  );
  const isSelected = (s: Suggestion) => overrides[s.key] ?? s.recommended;
  const selected = $derived(items.filter((s) => !meltingRows[s.key] && isSelected(s)));
  const freeable = $derived(selected.reduce((n, s) => n + s.memory, 0));

  const levelText = { smooth: "Running smooth", busy: "Getting busy", strained: "Under strain" } as const;

  // Numbers glide to their new values instead of jumping.
  const memTween = new Tween(0, { easing: butterEase });
  const cpuTween = new Tween(0, { easing: butterEase });
  const freeTween = new Tween(0, { easing: butterEase });
  $effect(() => {
    if (!scan) return;
    memTween.set(scan.stats.memUsed, { duration: ms(900) });
    cpuTween.set(scan.stats.cpu, { duration: ms(900) });
  });
  $effect(() => {
    freeTween.set(freeable, { duration: ms(500) });
  });

  function say(msg: string) {
    toast = msg;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 6000);
  }

  /** Docker/Ollama checks take several Docker calls: at most one at a time,
   * and the 15 s auto-refresh only redoes them once a minute. */
  const SERVICES_EVERY_MS = 60_000;
  let servicesBusy = false;
  let servicesAt = -Infinity;

  function refreshServices(force: boolean) {
    if (servicesBusy || (!force && performance.now() - servicesAt < SERVICES_EVERY_MS)) return;
    servicesBusy = true;
    // Docker stats can take a few seconds; don't hold the list for them.
    api
      .scanServices()
      .then((r) => (services = r))
      .catch(() => (services = null))
      .finally(() => {
        servicesBusy = false;
        servicesAt = performance.now();
        void loadSweep();
      });
  }

  /** `auto`: the periodic refresh, which may skip the Docker/Ollama check. */
  async function refresh(auto = false) {
    if (refreshing) return;
    refreshing = true;
    error = null;
    refreshServices(!auto);
    try {
      scan = await api.scan();
      void loadSweep();
      if (firstFill) setTimeout(() => (firstFill = false), 1200);
    } catch (e) {
      error = String(e);
    } finally {
      refreshing = false;
    }
  }

  async function loadSweep() {
    sweepIds = await api.sweepTargets().catch(() => []);
  }

  async function closeOne(s: Suggestion, force: boolean): Promise<Outcome> {
    pending[s.key] = true;
    try {
      const out = await api.close(s.id, force);
      results[s.key] = out;
      return out;
    } catch (e) {
      const out: Outcome = { status: "failed", message: String(e) };
      results[s.key] = out;
      return out;
    } finally {
      pending[s.key] = false;
    }
  }

  const MELT_MS = 1000;
  const sleep = (n: number) => new Promise((r) => setTimeout(r, n));

  /** Plays the melt on a closed row, then lets it leave the list. */
  /** Drops fall from the melting row and splash on the footer's top edge. */
  function pour_drops(key: string) {
    const row = document.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`);
    if (!row || !dropLayer || !footerEl) return;
    dropFrom(dropLayer, row.getBoundingClientRect(), footerEl.getBoundingClientRect());
  }

  /** Starts one row's melt (it stays in the list until `release`). */
  function startMelt(s: Suggestion) {
    meltingRows[s.key] = s;
    melting[s.key] = true;
    pending[s.key] = false;
    api.sound("melt");
    pour_drops(s.key);
  }

  /** The melted rows leave the list together, in one smooth reflow. */
  function release(keys: string[]) {
    for (const k of keys) {
      gone[k] = true; // instant exit: the melt already played
      delete melting[k];
      delete meltingRows[k];
    }
  }

  function meltAway(s: Suggestion) {
    startMelt(s);
    setTimeout(() => release([s.key]), ms(MELT_MS));
  }

  /** Rows that finish together still melt a beat apart, never in one lump. */
  let nextMeltAt = 0;
  async function meltInTurn(s: Suggestion) {
    pending[s.key] = true; // stays warm until its melt begins
    const now = performance.now();
    const at = Math.max(now, nextMeltAt);
    nextMeltAt = at + ms(90);
    if (at > now) await sleep(at - now);
    meltAway(s);
  }

  function celebrate(closed: Suggestion[], outs: Outcome[]) {
    const freed = outs.reduce((n, o) => n + (o.status === "closed" ? o.freed : 0), 0);
    const cpu = closed.reduce((n, s) => n + s.cpu, 0);
    const ports = closed.reduce((n, s) => n + s.ports.length, 0);
    savings = { freed, closed: closed.length, cpu, ports };
    api.sound("freed");
    void loadSmoothness();
  }

  /** Runs a close flow with the panel held open, then gives focus back. */
  async function held<T>(work: () => Promise<T>): Promise<T> {
    api.hold(true);
    try {
      return await work();
    } finally {
      api.hold(false);
    }
  }

  function closeSelected() {
    return held(closeSelectedNow);
  }

  async function closeSelectedNow() {
    confirming = false;
    running = true;
    const batch = [...selected];
    const closed: Suggestion[] = [];
    const outs: Outcome[] = [];
    // Every chosen row warms the instant Close is pressed, all of them are
    // closed at once (in parallel), and each melts the moment it's done: a
    // quick dev server doesn't wait for an app that's still saving.
    for (const s of batch) pending[s.key] = true;
    const closeAll = (group: Suggestion[]) =>
      Promise.all(
        group.map(async (s) => {
          const out = await closeOne(s, false);
          if (out.status === "closed") {
            closed.push(s);
            outs.push(out);
            delete overrides[s.key];
            await meltInTurn(s);
          }
        }),
      );
    // Docker itself goes last: its containers and clean-ups need Docker's
    // engine running, and quitting Docker would stop it mid-way.
    const runtimes = batch.filter((s) => s.kind === "container_runtime");
    await closeAll(batch.filter((s) => s.kind !== "container_runtime"));
    await closeAll(runtimes);
    if (closed.length) {
      await sleep(ms(MELT_MS * 0.7));
      celebrate(closed, outs);
      // Footer stays on "Closing…" and the list isn't refreshed until the
      // melted rows have left, so nothing reshuffles under the animation.
      await sleep(ms(MELT_MS * 0.35));
    } else {
      say("Nothing closed");
    }
    running = false;
    await refresh();
  }

  function force(s: Suggestion) {
    return held(() => forceNow(s));
  }

  async function forceNow(s: Suggestion) {
    const out = await closeOne(s, true);
    if (out.status === "closed") {
      meltAway(s);
      await sleep(ms(MELT_MS * 0.7));
      celebrate([s], [out]);
    } else {
      say(out.message);
    }
    await refresh();
  }

  function closeProtected(p: Protected) {
    return held(() => closeProtectedNow(p));
  }

  async function closeProtectedNow(p: Protected) {
    if (!p.id) return;
    armedKey = null;
    pending[p.key] = true;
    let out: Outcome;
    try {
      out = await api.close(p.id, false);
    } catch (e) {
      out = { status: "failed", message: String(e) };
    } finally {
      pending[p.key] = false;
    }
    if (out.status === "closed") {
      melting[p.key] = true;
      api.sound("melt");
      pour_drops(p.key);
      setTimeout(() => {
        gone[p.key] = true;
        delete melting[p.key];
      }, ms(MELT_MS));
      await sleep(ms(MELT_MS * 0.7));
      savings = { freed: out.freed, closed: 1, cpu: p.cpu, ports: 0 };
      api.sound("freed");
    } else {
      say(out.message);
    }
    await refresh();
  }

  /** Knife gesture: slice one row off (polite close, like Close). */
  function slice(s: Suggestion) {
    return held(() => sliceNow(s));
  }

  async function sliceNow(s: Suggestion) {
    const out = await closeOne(s, false);
    if (out.status === "closed") {
      delete overrides[s.key];
      meltAway(s);
      await sleep(ms(MELT_MS * 0.7));
      celebrate([s], [out]);
    }
    await refresh();
  }

  function openStats(tab: "life" | "fan" = "life") {
    statsTab = tab;
    view = "stats";
    void loadSmoothness();
  }

  async function loadSmoothness() {
    smooth = await api.smoothness().catch(() => smooth);
  }

  /** Sweep: tick exactly the safe set, then use the normal Close confirm. */
  function startSweep() {
    const ids = new Set(sweepIds);
    for (const s of items) overrides[s.key] = ids.has(s.id);
    confirming = true;
    api.sound("tick");
  }

  async function checkUpdate() {
    update = update ? { ...update, checking: true, error: null } : update;
    update = await api.checkUpdate().catch(() => update);
  }

  async function installUpdate() {
    updating = true;
    try {
      await api.installUpdate(); // Butters restarts when this succeeds
    } catch (e) {
      say(String(e));
      update = await api.updateStatus().catch(() => update);
    } finally {
      updating = false;
    }
  }

  async function ignore(s: Suggestion) {
    try {
      await api.ignore(s.id);
      say(`Ignoring ${s.title}`);
      await refresh();
    } catch (e) {
      say(String(e));
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (savings) savings = null;
    else if (armedKey) armedKey = null;
    else if (confirming) confirming = false;
    else if (view !== "main") view = "main";
    else api.hide();
  }

  // Forget notes for things that no longer show up.
  $effect(() => {
    const keys = new Set(items.map((s) => s.key));
    for (const k of Object.keys(results)) if (!keys.has(k) && !pending[k]) delete results[k];
    // Once a scan no longer lists something, it can't come back as "gone".
    const listed = new Set([...(scan?.suggestions ?? []), ...(services?.suggestions ?? [])].map((s) => s.key));
    for (const k of Object.keys(gone)) if (!listed.has(k) && !meltingRows[k]) delete gone[k];
  });

  /** Live meters: memory and CPU every 2 s while the panel is open. */
  async function tickLive() {
    if (document.visibilityState !== "visible" || !scan) return;
    try {
      const live = await api.live();
      scan.stats = live.stats;
      scan.level = live.level;
    } catch {
      /* next tick will try again */
    }
  }

  async function fixNotify() {
    notify = await api.fixNotifyPermission().catch(() => notify);
  }

  onMount(() => {
    // Docker/Ollama results from the background check show at once, together
    // with everything else; a fresh check then updates them quietly.
    api
      .cachedServices()
      .then((r) => {
        if (r && !services) services = r;
      })
      .catch(() => {});
    api
      .isPlayground()
      .then(async (yes) => {
        if (yes) playground = await api.playgroundStatus();
      })
      .catch(() => {});
    api.sound("open");
    api
      .notifyPermission()
      .then((n) => (notify = n))
      .catch(() => {});
    const live = setInterval(tickLive, 2000);
    // Start with focus on the panel itself, not on the first checkbox,
    // so no control shows a focus ring until the keyboard is used.
    panelEl.focus({ preventScroll: true });
    refresh();
    api
      .updateStatus()
      .then((u) => (update = u))
      .catch(() => {});
    void loadSmoothness();
    const timer = setInterval(() => {
      if (!running && !confirming && !savings && view === "main" && document.visibilityState === "visible")
        refresh(true);
    }, 15000);
    return () => {
      clearInterval(live);
      clearInterval(timer);
      clearTimeout(toastTimer);
    };
  });

  const memPct = $derived(scan ? (memTween.current / Math.max(1, scan.stats.memTotal)) * 100 : 0);
  const cpuPct = $derived(cpuTween.current);
  const memState = $derived(
    scan?.stats.pressure === "critical" || memPct >= 92
      ? "bad"
      : scan?.stats.pressure === "warn" || memPct >= 85
        ? "warn"
        : "ok",
  );
  const cpuState = $derived(cpuPct >= 85 ? "bad" : cpuPct >= 60 ? "warn" : "ok");
  const heat = $derived(
    scan?.stats.thermal === "serious" || scan?.stats.thermal === "critical"
      ? "Running hot, throttling"
      : scan?.stats.thermal === "fair"
        ? "Running warm"
        : null,
  );
  // Texture warmth: 0 cool and calm, 1 when the machine is running hot.
  const warmth = $derived(
    scan?.stats.thermal === "serious" || scan?.stats.thermal === "critical" || scan?.level === "strained"
      ? 1
      : scan?.stats.thermal === "fair" || scan?.level === "busy"
        ? 0.5
        : 0,
  );
  const footState = $derived(running ? "running" : confirming ? "confirm" : selected.length ? "ready" : "idle");
</script>

<svelte:window onkeydown={onKey} />

<main class="panel" tabindex="-1" bind:this={panelEl} style:--warmth={warmth}>
  {#if view === "main"}
    <section class="view" aria-label="Butters" in:dissolve={{ duration: 220, blur: 3 }} out:dissolve={{ duration: 160, blur: 3 }}>
      <header class="head">
        <div class="status">
          <StatusPat level={scan?.level ?? 'smooth'} />
          {#key scan?.level ?? "checking"}
            <h1 in:dissolve>{scan ? levelText[scan.level] : "Checking…"}</h1>
          {/key}
        </div>
        <div class="tools">
          <button class="ghost" type="button" aria-label="Refresh" use:tip={"Refresh"} onclick={() => refresh()} disabled={refreshing || running}>
            <span class="spin" class:spinning={refreshing}><Icon name="refresh" /></span>
          </button>
          <button class="ghost" type="button" aria-label="Stats" use:tip={"Stats"} onclick={() => openStats()}>
            <Icon name="chart" />
          </button>
          <button class="ghost" type="button" aria-label="Settings" use:tip={"Settings"} onclick={() => (view = "settings")}>
            <Icon name="settings" />
          </button>
        </div>
      </header>

      <div class="meters" aria-label="System">
        <ButterGauges
          memUsed={scan ? memTween.current : 0}
          memTotal={scan?.stats.memTotal ?? 0}
          cpu={cpuPct}
          {memState}
          cpuState={cpuState as "ok" | "warn" | "bad"}
        />
        {#if heat || cpuState === "bad"}
          <button class="heat" type="button" in:pour onclick={() => openStats("fan")}>{heat ?? "CPU is pinned"} · see why</button>
        {/if}
      </div>

      {#if playground}
        <div class="playground" in:pour={{ y: -4 }}>
          <span class="pg-text">Playground · {playground.node ? `${playground.running} of ${playground.total} test items running` : "Needs Node.js"}</span>
          <button class="btn small" type="button" onclick={resetPlayground} disabled={resetting || !playground.node}>{resetting ? "Resetting…" : "Reset"}</button>
        </div>
      {/if}

      {#if notify !== "on"}
        <div class="update" in:pour={{ y: -4 }} out:melt>
          <span class="update-text">{notify === "ask" ? "Turn on alerts" : "Alerts are off in Settings"}</span>
          <button class="btn small primary-soft" type="button" onclick={fixNotify}>{notify === "ask" ? "Turn on" : "Open"}</button>
        </div>
      {/if}

      {#if update?.available}
        <div class="update" in:pour={{ y: -4 }} out:melt>
          <span class="update-text">Update {update.available} ready</span>
          <button class="btn small primary-soft" type="button" onclick={installUpdate} disabled={updating}>
            {updating ? "Updating…" : "Update"}
          </button>
        </div>
      {/if}

      <div class="scroll">
        {#if error}
          <div class="empty" in:pour>
            <p class="empty-title" use:tip={error}>Couldn't read processes</p>
            <button class="btn" type="button" onclick={() => refresh()}>Try again</button>
          </div>
        {:else if !scan}
          <div class="skeleton" aria-label="Looking at what's running" role="status">
            {#each [0, 1, 2] as i (i)}<div class="pat" style:animation-delay="{i * 140}ms"></div>{/each}
          </div>
        {:else}
          <div class="section-row">
            <h2 class="section">
              {items.length ? `Could close · ${items.length}` : "Nothing to close"}
            </h2>
            {#if sweepIds.length >= 2 && !running}
              <button class="link-btn" type="button" use:tip={{ text: "Sweep", lines: ["Leftovers, idle agents, idle containers", "You confirm first"] }} onclick={startSweep} in:dissolve>
                Sweep {sweepIds.length}
              </button>
            {/if}
          </div>
          {#if items.length}
            <ul class="list">
              {#each items as s, i (s.key)}
                <li
                  data-key={s.key}
                  animate:flip={{ duration: ms(620), easing: butterEase }}
                  in:pour={{ delay: firstFill ? 80 + i * 55 : 0, blur: 0 }}
                  out:melt={{ duration: gone[s.key] ? 0 : 760, blur: 0 }}
                >
                  <Row
                    {s}
                    selected={isSelected(s)}
                    melting={!!melting[s.key]}
                    pending={!!pending[s.key]}
                    result={results[s.key]}
                    onToggle={(on) => {
                      overrides[s.key] = on;
                      api.sound("tick");
                    }}
                    onIgnore={() => ignore(s)}
                    onForce={() => force(s)}
                    onSlice={() => slice(s)}
                  />
                </li>
              {/each}
            </ul>
          {:else}
            {#if scan.level === "smooth" && !savings}
              <Toast />
            {/if}
            {#if scan.watchedSecs < 3600}<p class="empty-note muted" in:pour>Idle apps show after 1 h</p>{/if}
          {/if}

          {#if scan.protected.length}
            <ProtectedList
              items={scan.protected}
              bind:open={showProtected}
              bind:armedKey
              {gone}
              {melting}
              {pending}
              onClose={closeProtected}
            />
          {/if}
        {/if}
      </div>

      <footer class="bar" bind:this={footerEl}>
        {#key footState}
          <div class="bar-inner" in:dissolve={{ delay: 60 }} out:dissolve={{ duration: 160 }}>
            {#if footState === "running"}
              <p class="bar-text softening">Closing…</p>
            {:else if footState === "confirm"}
              <p class="bar-text">Close {selected.length}?</p>
              <div class="bar-actions">
                <button class="btn" type="button" onclick={() => (confirming = false)}>Cancel</button>
                <button class="btn primary" type="button" onclick={closeSelected}>Close</button>
              </div>
            {:else if footState === "ready"}
              <p class="bar-text">Frees <strong>{bytes(freeTween.current)}</strong></p>
              <button class="btn primary" type="button" onclick={() => (confirming = true)}>Close {selected.length}</button>
            {:else}
              {#if smooth && (smooth.score !== null || smooth.freedWeek > 0)}
                <button class="bar-text smooth-line" type="button" use:tip={"See everything Butters has freed"} onclick={() => openStats()}>
                  {#if smooth.score !== null}<span>Buttery <strong>{smooth.score}</strong></span>{/if}
                  {#if smooth.freedWeek > 0}<span class="muted">· {bytes(smooth.freedWeek)} this week</span>{/if}
                </button>
                <svg class="spark" viewBox="0 0 48 16" aria-hidden="true">
                  {#each smooth.week as v, i (i)}
                    {@const max = Math.max(1, ...smooth.week)}
                    <rect x={i * 7} y={16 - Math.max(1.5, (v / max) * 16)} width="5" height={Math.max(1.5, (v / max) * 16)} rx="2" class:today={i === 6} />
                  {/each}
                </svg>
              {:else}
                <p class="bar-text muted">Tick to close</p>
              {/if}
            {/if}
          </div>
        {/key}
      </footer>
    </section>
  {:else if view === "stats"}
    <StatsView {smooth} tab={statsTab} onBack={() => (view = "main")} />
  {:else}
    <SettingsView
      bind:settings
      {update}
      {updating}
      onInstall={installUpdate}
      onCheck={checkUpdate}
      onBack={() => (view = "main")}
    />
  {/if}

  {#if savings}
    <Savings {...savings} total={(smooth?.lifetime.freed ?? 0) + savings.freed} onDone={() => (savings = null)} />
  {/if}

  <div class="toast-slot" role="status" aria-live="polite">
    {#if toast}
      {#key toast}<p class="toast" in:pour={{ y: 8 }} out:melt={{ duration: 420 }}>{toast}</p>{/key}
    {/if}
  </div>
  <div class="drop-layer" bind:this={dropLayer} aria-hidden="true"></div>
  <!-- Drips that run off the bottom edge when the panel melts away. -->
  <span class="panel-drips" aria-hidden="true"><i></i><i></i><i></i><i></i></span>
</main>

<style>
  .panel {
    position: relative;
    height: 100%;
    display: grid;
    background: var(--bg);
    color: var(--ink);
    overflow: hidden;
    animation: pour-in 560ms var(--butter-ease) both;
    transform-origin: 50% 0%;
  }
  /* Texture: a fine paper grain (one static image, painted once) and a soft
     butter sheen from the top-left. Both warm up as the machine heats. */
  .panel::before,
  .panel::after {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    border-radius: inherit;
    z-index: 0;
  }
  .panel::before {
    background-image: url("data:image/svg+xml;utf8,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='140'%20height='140'%3E%3Cfilter%20id='n'%3E%3CfeTurbulence%20type='fractalNoise'%20baseFrequency='.9'%20numOctaves='2'%20stitchTiles='stitch'/%3E%3CfeColorMatrix%20values='0%200%200%200%20.45%20%200%200%200%200%20.33%20%200%200%200%200%20.12%20%200%200%200%20.55%200'/%3E%3C/filter%3E%3Crect%20width='100%'%20height='100%'%20filter='url(%23n)'/%3E%3C/svg%3E");
    background-size: 140px 140px;
    opacity: 0.07;
    mix-blend-mode: multiply;
  }
  .panel::after {
    background:
      radial-gradient(120% 60% at 0% 0%, rgba(255, 236, 170, calc(0.32 + var(--warmth, 0) * 0.18)), transparent 60%),
      radial-gradient(90% 50% at 100% 100%, rgba(232, 150, 60, calc(var(--warmth, 0) * 0.12)), transparent 70%);
    transition: opacity 1.2s var(--butter-ease);
  }
  /* Content sits above the texture; overlays keep their own positioning. */
  /* Every view sits in the same grid cell, so switching views cross-fades
     in place instead of stacking the new one below the leaving one. */
  .view,
  .panel > :global(.view) {
    position: relative;
    z-index: 1;
    grid-area: 1 / 1;
    min-height: 0;
  }

  /* Physics melt: drops and the footer puddle */
  .drop-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 4;
    overflow: hidden;
    border-radius: inherit;
  }
  .drop-layer :global(.drop) {
    position: absolute;
    left: 0;
    top: 0;
    border-radius: 50% 50% 50% 50% / 60% 60% 40% 40%;
    background: radial-gradient(circle at 35% 30%, #fff5d6, var(--butter) 55%, var(--butter-deep));
    will-change: transform, opacity;
  }
  .smooth-line {
    border: 0;
    padding: 0;
    background: none;
    font: inherit;
    color: inherit;
    cursor: pointer;
    text-align: start;
    display: flex;
    gap: 6px;
    align-items: baseline;
  }
  .spark {
    width: 48px;
    height: 16px;
    flex: none;
  }
  .spark rect {
    fill: var(--butter);
    opacity: 0.55;
  }
  .spark rect.today {
    fill: var(--butter-deep);
    opacity: 1;
  }

  /* The panel itself is only a focus anchor for Escape and Tab; it is not a control. */
  .panel:focus-visible {
    outline: 2px solid transparent;
  }
  :global(html.floating) .panel {
    height: calc(100% - 24px);
    margin: 4px 12px 20px;
    border-radius: var(--r-panel);
    box-shadow: var(--shadow), 0 0 0 1px var(--frame);
  }
  /* Clicking away: a quick melt. Contents soften at once while the panel
     slumps, spreads a touch and drips, then it's gone. 260 ms, matched by
     LEAVE in panel.rs and the "close" sound. */
  :global(html.leaving) .panel {
    animation: panel-melt 260ms cubic-bezier(0.4, 0, 0.8, 0.4) forwards;
    transform-origin: 50% 0%;
    overflow: visible;
    pointer-events: none;
  }
  :global(html.leaving) .panel > :global(*:not(.panel-drips)) {
    animation: panel-content-melt 140ms var(--butter-ease) forwards;
  }
  :global(html.leaving) .panel-drips {
    opacity: 1;
  }
  :global(html.leaving) .panel-drips i {
    animation: panel-drip 260ms cubic-bezier(0.5, 0, 0.6, 1) forwards;
  }
  @keyframes panel-melt {
    0% {
      transform: none;
      filter: none;
      opacity: 1;
    }
    40% {
      transform: translateY(6px) scale(1.02, 0.9);
      filter: blur(1px);
      opacity: 0.9;
    }
    100% {
      transform: translateY(24px) scale(1.06, 0.6);
      filter: blur(6px);
      opacity: 0;
    }
  }
  @keyframes panel-content-melt {
    to {
      opacity: 0;
      filter: blur(5px);
      transform: translateY(6px);
    }
  }
  .panel-drips {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 0;
    opacity: 0;
    pointer-events: none;
    z-index: 6;
  }
  .panel-drips i {
    position: absolute;
    top: -6px;
    width: 9px;
    height: 22px;
    border-radius: 0 0 5px 5px;
    background: linear-gradient(var(--surface), var(--butter-hi));
    transform-origin: 50% 0;
    transform: scaleY(0);
  }
  .panel-drips i:nth-child(1) {
    left: 16%;
  }
  .panel-drips i:nth-child(2) {
    left: 38%;
    width: 7px;
    animation-delay: 20ms !important;
  }
  .panel-drips i:nth-child(3) {
    left: 61%;
    width: 11px;
    animation-delay: 10ms !important;
  }
  .panel-drips i:nth-child(4) {
    left: 83%;
    width: 7px;
    animation-delay: 35ms !important;
  }
  @keyframes panel-drip {
    0% {
      transform: scaleY(0);
    }
    100% {
      transform: scaleY(1.6);
    }
  }
  @keyframes pour-in {
    from {
      opacity: 0;
      filter: blur(10px);
      transform: translateY(-10px) scale(0.985);
    }
    to {
      opacity: 1;
      filter: blur(0);
      transform: none;
    }
  }

  .view {
    grid-area: 1 / 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px var(--pad) 10px;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  h1 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    letter-spacing: -0.025em;
    line-height: 1.3;
  }

  .tools {
    display: flex;
    gap: 2px;
  }
  .ghost {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: var(--r-control);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    transition:
      color var(--t-soft) var(--butter-ease),
      background-color var(--t-soft) var(--butter-ease),
      transform var(--t-quick) var(--butter-ease);
  }
  .ghost:hover:not(:disabled) {
    color: var(--ink);
    background: var(--surface-soft);
  }
  .ghost:active:not(:disabled) {
    transform: scale(0.9);
  }
  .ghost:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .spin {
    display: grid;
  }
  .spinning {
    animation: spin 1100ms var(--butter-ease) infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .meters {
    display: grid;
    gap: 6px;
    padding: 0 var(--pad) 12px;
  }
  .heat {
    grid-column: 1 / -1;
    justify-self: start;
    margin: 6px 0 0;
    padding: 0 0 0 8px;
    border: 0;
    border-inline-start: 2px solid var(--danger-fill);
    background: none;
    color: var(--ink);
    font: inherit;
    font-size: 12px;
    text-align: start;
    white-space: nowrap;
    cursor: pointer;
  }
  .heat:hover {
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .section-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .link-btn {
    margin-inline-end: var(--inset);
    padding: 2px 8px;
    border: 0;
    border-radius: 10px;
    background: var(--surface-soft);
    color: var(--ink);
    font: inherit;
    font-size: 11.5px;
    font-weight: 600;
    line-height: 1.3;
    cursor: pointer;
    transition: background-color var(--t-soft) var(--butter-ease);
  }
  .link-btn:hover {
    background: var(--butter-hi);
  }

  .playground {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin: 0 var(--pad) 8px;
    padding: 6px 6px 6px 12px;
    border-radius: var(--r-pat);
    background: repeating-linear-gradient(-45deg, var(--surface-soft) 0 8px, var(--surface-picked) 8px 16px);
    color: var(--ink);
    font-size: 12px;
    font-weight: 600;
  }
  .pg-text {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 0 var(--inset) 16px;
    /* Content softens into the edges instead of being cut off. */
    mask-image: linear-gradient(to bottom, transparent 0, #000 10px, #000 calc(100% - 18px), transparent 100%);
    scrollbar-width: thin;
  }
  .section {
    margin: 0;
    padding: 10px var(--inset) 6px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--muted);
    letter-spacing: 0;
  }
  .list {
    position: relative;
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .empty,
  .empty-note {
    padding: 10px var(--inset);
    margin: 0;
  }
  .empty p {
    margin: 0 0 8px;
  }
  .empty-title {
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
  }

  .skeleton {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 0;
  }
  .pat {
    height: 52px;
    border-radius: var(--r-pat);
    background: linear-gradient(100deg, var(--surface-soft) 30%, var(--surface-picked) 50%, var(--surface-soft) 70%);
    background-size: 300% 100%;
    animation: sheen 1800ms var(--butter-ease) infinite;
  }
  @keyframes sheen {
    from {
      background-position: 100% 0;
    }
    to {
      background-position: 0 0;
    }
  }

  .bar-text,
  .toast,
  .heat {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bar {
    position: relative;
    display: grid;
    min-height: 58px;
    padding: 12px var(--pad);
    background: var(--surface);
    color: var(--ink);
  }
  .bar-inner {
    grid-area: 1 / 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--gap);
  }
  .bar-text {
    margin: 0;
    min-width: 0;
    font-size: 12px;
  }
  .bar-text strong {
    font-weight: 600;
  }
  .softening {
    color: var(--muted);
  }
  .bar-actions {
    display: flex;
    gap: 8px;
  }
  .btn {
    height: var(--control-h);
    padding: 0 14px;
    border: 1px solid var(--hairline);
    border-radius: var(--r-control);
    background: var(--surface);
    color: var(--ink);
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--t-soft) var(--butter-ease),
      background-position var(--t-slow) var(--butter-ease),
      transform var(--t-quick) var(--butter-ease);
  }
  .btn:hover {
    background-color: var(--surface-soft);
    color: var(--ink);
  }
  .btn:active {
    transform: scale(0.97);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  /* The one butter button: a soft, glossy pat. */
  .btn.primary {
    border-color: var(--butter-edge);
    background-image: linear-gradient(180deg, var(--butter-hi) 0%, var(--butter) 50%, var(--butter-hi) 100%);
    background-size: 100% 200%;
    background-position: 0 0;
    color: #2e2820;
    font-weight: 600;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.55), 0 1px 2px rgba(120, 86, 20, 0.18);
  }
  .btn.primary:hover {
    background-position: 0 100%;
    color: #2e2820;
  }

  .toast-slot {
    position: absolute;
    left: var(--pad);
    right: var(--pad);
    bottom: 74px;
    display: grid;
    pointer-events: none;
  }
  .toast {
    grid-area: 1 / 1;
    justify-self: center;
    margin: 0;
    padding: 9px 14px;
    border-radius: var(--r-pat);
    background: var(--toast-bg);
    color: var(--toast-ink);
    font-size: 12px;
    font-weight: 500;
    line-height: 1.35;
    box-shadow: var(--shadow);
    max-width: 100%;
  }

  .update {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--gap);
    margin: 0 var(--pad) 10px;
    padding: 6px 6px 6px 12px;
    border-radius: var(--r-pat);
    background: var(--butter-hi);
    color: var(--ink);
  }
  .update-text {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .btn.small {
    height: 28px;
    padding: 0 12px;
  }
  .btn.primary-soft {
    background: var(--surface);
    border-color: var(--butter-edge);
    color: var(--ink);
    font-weight: 600;
  }

  @media (prefers-reduced-motion: reduce) {
    .panel {
      animation: none;
    }
    :global(html.leaving) .panel {
      animation: none;
      opacity: 0;
      transition: opacity 120ms linear;
    }
    :global(html.leaving) .panel > :global(*:not(.panel-drips)) {
      animation: none;
    }
    .panel-drips {
      display: none;
    }
    .ghost:active:not(:disabled),
    .btn:active,
    .spinning {
      transform: none;
      animation: none;
    }
    .pat {
      animation: none;
    }
  }
</style>
