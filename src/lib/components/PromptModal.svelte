<script lang="ts" module>
  export type TplPrompt = { kind: "prompt" | "suggester"; label: string; default: string; options: string[]; values: string[] };
</script>

<script lang="ts">
  // Perguntas do Templater (tp.system.prompt / tp.system.suggester), uma tela para todas.
  import { tick, untrack } from "svelte";

  type Props = { title: string; prompts: TplPrompt[]; onsubmit: (answers: string[]) => void; onclose: () => void };
  let { title, prompts, onsubmit, onclose }: Props = $props();

  let answers = $state(untrack(() => prompts.map((p) => (p.kind === "suggester" ? (p.values[0] ?? "") : p.default))));
  let first = $state<HTMLElement>();
  $effect(() => {
    tick().then(() => first?.focus());
  });

  function submit(e?: Event) {
    e?.preventDefault();
    onclose();
    onsubmit(answers);
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="sx-overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <form class="sx-prompt" onsubmit={submit}>
    <div class="sx-prompt-title">{title}</div>
    {#each prompts as p, i}
      <label class="mesa-field is-wide">
        <span>{p.label}</span>
        {#if p.kind === "suggester"}
          {#if i === 0}
            <select class="sx-input" bind:this={first} bind:value={answers[i]}>
              {#each p.options as opt, j}<option value={p.values[j] ?? opt}>{opt}</option>{/each}
            </select>
          {:else}
            <select class="sx-input" bind:value={answers[i]}>
              {#each p.options as opt, j}<option value={p.values[j] ?? opt}>{opt}</option>{/each}
            </select>
          {/if}
        {:else if i === 0}
          <input class="sx-input" bind:this={first} bind:value={answers[i]} />
        {:else}
          <input class="sx-input" bind:value={answers[i]} />
        {/if}
      </label>
    {/each}
    <div class="mesa-form-actions">
      <button type="button" class="btn-ghost" onclick={onclose}>Cancelar</button>
      <button type="submit" class="btn-primary">Inserir</button>
    </div>
  </form>
</div>
