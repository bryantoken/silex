<script lang="ts">
  // Logo do Silex: pedra de sílex lapidada. Sem fundo e na cor do texto do tema
  // (branca no escuro, escura no claro); `tile` desenha o ícone do app (pedra
  // branca sobre quadrado cinza). Fonte: assets/logo.svg / src/lib/logo.ts
  import { LOGO_FACETS } from "../logo";
  let { size = 28, tile = false }: { size?: number; tile?: boolean } = $props();
</script>

<svg class="sx-logo" width={size} height={size} viewBox="0 0 1024 1024" role="img" aria-label="Silex">
  {#if tile}
    <rect width="1024" height="1024" rx="232" fill="#202020" />
  {/if}
  <g fill={tile ? "#ffffff" : "currentColor"} transform={tile ? "translate(512 512) scale(0.82) translate(-512 -512)" : undefined}>
    {#each LOGO_FACETS as f}<polygon points={f.points} fill-opacity={f.opacity} />{/each}
  </g>
</svg>
