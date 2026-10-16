<script lang="ts" module>
  export type SavedVault = { path: string; name: string; last_opened: number };
</script>

<script lang="ts">
  import Logo from "./Logo.svelte";
  import Icon from "./Icon.svelte";

  type Props = {
    vaults: SavedVault[];
    current: string | null;
    onopen: (path: string) => void;
    onforget: (path: string) => void;
    onpick: () => void;
    onclose?: () => void;
  };
  let { vaults, current, onopen, onforget, onpick, onclose }: Props = $props();

  function ago(secs: number) {
    const d = Date.now() / 1000 - secs;
    if (d < 60) return "agora";
    if (d < 3600) return `há ${Math.floor(d / 60)} min`;
    if (d < 86400) return `há ${Math.floor(d / 3600)} h`;
    if (d < 86400 * 30) return `há ${Math.floor(d / 86400)} dias`;
    return new Date(secs * 1000).toLocaleDateString("pt-BR");
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose?.()} />

<div class="sx-vaults" class:is-modal={!!onclose}>
  <div class="sx-vaults-card">
    {#if onclose}
      <button class="icon-btn sx-vaults-close" title="Fechar" onclick={onclose}><Icon name="x" size={16} /></button>
    {/if}
    <div class="sx-vaults-brand">
      <Logo size={56} />
      <div>
        <h1>Silex</h1>
        <p>Suas notas, em velocidade de Rust.</p>
      </div>
    </div>

    {#if vaults.length}
      <div class="sx-vaults-label">Cofres recentes</div>
      <div class="sx-vaults-list">
        {#each vaults as v (v.path)}
          <div class="sx-vault-row" class:is-current={v.path === current}>
            <button class="sx-vault-open" onclick={() => onopen(v.path)} title={v.path}>
              <span class="sx-vault-avatar">{v.name.slice(0, 1).toUpperCase()}</span>
              <span class="sx-vault-text">
                <span class="name">{v.name}{#if v.path === current}<span class="badge">aberto</span>{/if}</span>
                <span class="path">{v.path}</span>
              </span>
              <span class="when">{ago(v.last_opened)}</span>
            </button>
            <button class="icon-btn sm" title="Remover da lista (não apaga arquivos)" onclick={() => onforget(v.path)}>
              <Icon name="x" size={14} />
            </button>
          </div>
        {/each}
      </div>
    {/if}

    <button class="btn-primary sx-vaults-pick" onclick={onpick}>
      <Icon name="folder-plus" size={16} /> Abrir pasta como cofre
    </button>
  </div>
</div>
