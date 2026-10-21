<script lang="ts">
  import { flip } from "svelte/animate";
  import type { Protected } from "./api";
  import { bytes } from "./format";
  import Icon from "./Icon.svelte";
  import { butterEase, dissolve, melt, ms, pour } from "./motion";

  let {
    items,
    open = $bindable(),
    armedKey = $bindable(),
    gone,
    melting,
    pending,
    onClose,
  }: {
    items: Protected[];
    /** Disclosure state, kept by the panel across view switches. */
    open: boolean;
    /** A protected app the user chose to close: first click arms, second confirms. */
    armedKey: string | null;
    gone: Record<string, boolean>;
    melting: Record<string, boolean>;
    pending: Record<string, boolean>;
    onClose: (p: Protected) => void;
  } = $props();
</script>

<div class="protected">
  <button class="disclose" type="button" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="chev" class:open><Icon name="chevron" size={14} /></span>
    Protected · {items.length}
  </button>
  {#if open}
    <ul in:pour={{ y: -4 }} out:dissolve={{ duration: 200 }}>
      {#each items.filter((x) => !gone[x.key]) as p (p.key)}
        <li
          data-key={p.key}
          class:closable={!!p.id}
          class:armed={armedKey === p.key}
          class:melting={!!melting[p.key]}
          animate:flip={{ duration: ms(620), easing: butterEase }}
          out:melt={{ duration: gone[p.key] ? 0 : 760, blur: 0 }}
        >
          <span class="p-title">{p.title}</span>
          <span class="p-reason muted">{p.reason}</span>
          <span class="p-end">
            {#if pending[p.key]}
              <span class="num softening" in:dissolve>…</span>
            {:else if armedKey === p.key}
              <!-- Second step: it's protected for a reason, so confirm. -->
              <button class="btn small danger-soft" type="button" in:dissolve={{ blur: 0 }} onclick={() => onClose(p)}>{p.verb}?</button>
            {:else}
              <span class="num p-mem">{bytes(p.memory)}</span>
              {#if p.id}
                <button class="btn small p-close" type="button" aria-label={`Close ${p.title}`} onclick={() => (armedKey = p.key)}>Close</button>
              {/if}
            {/if}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .protected {
    padding: 12px 0 0;
  }
  .disclose {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 28px;
    padding: 4px var(--inset);
    border: 0;
    border-radius: var(--r-control);
    background: transparent;
    color: var(--muted);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    transition: color var(--t-soft) var(--butter-ease), background-color var(--t-soft) var(--butter-ease);
  }
  .disclose:hover {
    color: var(--ink);
    background: var(--surface-soft);
  }
  .chev {
    display: grid;
    transition: transform var(--t-soft) var(--butter-ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .protected ul {
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
  }
  .protected li {
    display: grid;
    grid-template-columns: minmax(0, auto) minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 2px var(--inset);
    border-radius: var(--r-control);
    font-size: 12px;
    transition: background-color var(--t-soft) var(--butter-ease);
  }
  .protected li.closable:hover,
  .protected li.closable:focus-within,
  .protected li.armed {
    background: var(--surface-soft);
  }
  /* The memory figure gives way to a Close button on hover (and on keyboard
     focus), in the same spot, so the row never shifts. */
  .p-end {
    display: grid;
    justify-items: end;
    align-items: center;
    min-width: 64px;
  }
  .p-end > * {
    grid-area: 1 / 1;
  }
  /* Opacity and a small slide only: no CSS filters inside the list (WebKit
     draws stray magenta edges on blurred elements there). */
  .p-close {
    opacity: 0;
    transform: translateX(4px);
    pointer-events: none;
    transition:
      opacity var(--t-quick) var(--butter-ease),
      transform var(--t-quick) var(--butter-ease);
  }
  .p-mem {
    /* Only a label: clicks always reach the Close button underneath. */
    pointer-events: none;
    transition:
      opacity var(--t-quick) var(--butter-ease),
      transform var(--t-quick) var(--butter-ease);
  }
  li.closable:hover .p-close,
  .p-close:focus-visible {
    opacity: 1;
    transform: none;
    pointer-events: auto;
  }
  li.closable:hover .p-mem,
  li.closable:focus-within .p-mem {
    opacity: 0;
    transform: translateX(-4px);
  }
  .protected li.melting {
    pointer-events: none;
    transform-origin: 50% 100%;
    animation: p-melt 1000ms cubic-bezier(0.5, 0, 0.75, 0.25) forwards;
  }
  @keyframes p-melt {
    0% {
      background: var(--surface-soft);
    }
    25% {
      background: var(--butter-hi);
      transform: scale(1.01, 1.03);
    }
    70% {
      background: var(--butter);
      opacity: 1;
    }
    100% {
      background: var(--butter);
      transform: translateY(3px) scale(1.04, 0.1);
      opacity: 0;
    }
  }
  .p-title,
  .p-reason {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .p-title {
    font-weight: 600;
    max-width: 170px;
  }
  .num {
    color: var(--ink);
    font-weight: 500;
  }
  .softening,
  .muted {
    color: var(--muted);
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
  .btn.small.p-close,
  .btn.small.danger-soft {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }
  .btn.danger-soft {
    background: var(--surface);
    border-color: var(--danger);
    color: var(--danger);
    font-weight: 600;
  }

  @media (prefers-reduced-motion: reduce) {
    .btn:active {
      transform: none;
      animation: none;
    }
  }
</style>
