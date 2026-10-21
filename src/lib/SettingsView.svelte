<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Settings, type UpdateStatus } from "./api";
  import Check from "./Check.svelte";
  import Icon from "./Icon.svelte";
  import { dissolve, pour } from "./motion";
  import { tip } from "./tip";

  let {
    settings = $bindable(),
    update,
    updating,
    onInstall,
    onCheck,
    onBack,
  }: {
    /** Kept by the panel so reopening Settings shows the last values at once. */
    settings: Settings | null;
    update: UpdateStatus | null;
    updating: boolean;
    onInstall: () => void;
    onCheck: () => void;
    onBack: () => void;
  } = $props();

  let settingsError = $state<string | null>(null);

  onMount(async () => {
    try {
      settings = await api.getSettings();
    } catch (e) {
      settingsError = String(e);
    }
  });

  async function saveSettings(next: Settings) {
    settingsError = null;
    try {
      settings = await api.setSettings(next);
    } catch (e) {
      settingsError = String(e);
      settings = await api.getSettings().catch(() => settings);
    }
  }

  const isMac = /Mac/.test(navigator.platform) || /Macintosh/.test(navigator.userAgent);
  const SHORTCUTS = ["Alt+Super+B", "Control+Alt+B", "Alt+Super+Space", ""];
  function shortcutLabel(k: string) {
    if (!k) return "Off";
    const mac: Record<string, string> = { Alt: "⌥", Super: "⌘", Control: "⌃", Shift: "⇧", Space: "Space" };
    const win: Record<string, string> = { Alt: "Alt", Super: "Win", Control: "Ctrl", Shift: "Shift", Space: "Space" };
    const parts = k.split("+").map((p) => (isMac ? mac : win)[p] ?? p);
    return isMac ? parts.join("") : parts.join("+");
  }
</script>

<section class="view" aria-label="Settings" in:dissolve={{ duration: 220, blur: 3 }} out:dissolve={{ duration: 160, blur: 3 }}>
  <header class="head">
    <div class="status">
      <button class="ghost" type="button" aria-label="Back" use:tip={"Back"} onclick={onBack}><Icon name="back" /></button>
      <h1>Settings</h1>
    </div>
  </header>
  <div class="scroll settings">
    {#if settingsError}<p class="error" role="alert" in:pour>{settingsError}</p>{/if}
    {#if settings}
      {@const s = settings}
      <div class="setting" in:pour={{ delay: 40 }}>
        <label class="setting-text" for="set-alerts">
          <strong>Alerts</strong>
          <span class="muted">Low memory, heat, maxed CPU</span>
        </label>
        <Check id="set-alerts" variant="switch" checked={s.alerts} onchange={(on) => saveSettings({ ...s, alerts: on })} />
      </div>
      <div class="setting" in:pour={{ delay: 70 }}>
        <label class="setting-text" for="set-sounds">
          <strong>Sounds</strong>
          <span class="muted">Soft, in sync with animations</span>
        </label>
        <Check id="set-sounds" variant="switch" checked={s.sounds} onchange={(on) => saveSettings({ ...s, sounds: on }).then(() => on && api.sound("tick"))} />
      </div>
      <div class="setting" in:pour={{ delay: 85 }}>
        <label class="setting-text" for="set-shortcut">
          <strong>Shortcut</strong>
          <span class="muted">Open Butters from anywhere</span>
        </label>
        <select id="set-shortcut" class="select" value={s.shortcut} onchange={(e) => saveSettings({ ...s, shortcut: e.currentTarget.value })}>
          {#each SHORTCUTS as k (k)}
            <option value={k}>{shortcutLabel(k)}</option>
          {/each}
        </select>
      </div>
      <div class="setting" in:pour={{ delay: 100 }}>
        <label class="setting-text" for="set-login">
          <strong>Start at login</strong>
          <span class="muted">Lives in the menu bar</span>
        </label>
        <Check id="set-login" variant="switch" checked={s.launch_at_login} onchange={(on) => saveSettings({ ...s, launch_at_login: on })} />
      </div>
      <div class="setting" in:pour={{ delay: 160 }}>
        <span class="setting-text">
          <strong>Ignored</strong>
          <span class="muted">{s.ignored.length ? `${s.ignored.length} item${s.ignored.length === 1 ? "" : "s"}` : "None"}</span>
        </span>
        {#if s.ignored.length}<button class="btn" type="button" onclick={() => saveSettings({ ...s, ignored: [] })}>Clear</button>{/if}
      </div>
    {/if}
    <div class="setting" in:pour={{ delay: 200 }}>
      <span class="setting-text">
        <strong>Updates</strong>
        <span class="muted">
          {#if update?.installing || updating}Updating…
          {:else if update?.checking}Checking…
          {:else if update?.available}{update.available} ready
          {:else if update?.error}Couldn't check
          {:else}Version {update?.current ?? "0.1.0"}{/if}
        </span>
      </span>
      {#if update?.available}
        <button class="btn" type="button" onclick={onInstall} disabled={updating}>Update</button>
      {:else}
        <button class="btn" type="button" onclick={onCheck} disabled={update?.checking}>Check</button>
      {/if}
    </div>
    <div class="setting" in:pour={{ delay: 240 }}>
      <span class="setting-text">
        <strong>Quit Butters</strong>
      </span>
      <button class="btn" type="button" onclick={() => api.quit()}>Quit</button>
    </div>
  </div>
</section>

<style>
  .view {
    position: relative; /* above the panel texture */
    z-index: 1;
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
  .settings {
    padding-top: 4px;
  }
  .setting {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--gap);
    padding: 12px var(--inset);
    border-radius: var(--r-pat);
  }
  .setting-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .setting-text > * {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  label.setting-text {
    cursor: pointer;
  }
  .setting-text strong {
    font-weight: 600;
    font-size: 13px;
  }
  .muted {
    color: var(--muted);
  }
  .error {
    margin: 8px var(--inset);
    color: var(--danger);
  }
  .select {
    height: 28px;
    padding: 0 28px 0 10px;
    border: 1px solid var(--control-line);
    border-radius: var(--r-control);
    background:
      url("data:image/svg+xml;utf8,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6'%3E%3Cpath d='M1 1l4 4 4-4' fill='none' stroke='%236b6152' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E")
        no-repeat right 10px center,
      var(--surface);
    color: var(--ink);
    font: inherit;
    font-size: 12.5px;
    line-height: 1.25;
    appearance: none;
    -webkit-appearance: none;
    cursor: pointer;
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

  @media (prefers-reduced-motion: reduce) {
    .ghost:active:not(:disabled),
    .btn:active {
      transform: none;
      animation: none;
    }
  }
</style>
