// Tipos de callout (> [!tipo] Título) → cor e ícone. Tipos desconhecidos usam o acento.
type Kind = { color: string; icon: string; aliases: string[]; title: string };

const KINDS: Kind[] = [
  { title: "Nota", color: "hsl(212 90% 66%)", icon: "pencil", aliases: ["note"] },
  { title: "Resumo", color: "hsl(186 70% 56%)", icon: "clipboard", aliases: ["abstract", "summary", "tldr"] },
  { title: "Info", color: "hsl(212 90% 66%)", icon: "info", aliases: ["info"] },
  { title: "A fazer", color: "hsl(212 90% 66%)", icon: "check-circle", aliases: ["todo"] },
  { title: "Dica", color: "hsl(172 65% 52%)", icon: "flame", aliases: ["tip", "hint", "important"] },
  { title: "Sucesso", color: "hsl(146 60% 52%)", icon: "check", aliases: ["success", "check", "done"] },
  { title: "Pergunta", color: "hsl(42 95% 60%)", icon: "help", aliases: ["question", "help", "faq"] },
  { title: "Atenção", color: "hsl(28 95% 62%)", icon: "alert", aliases: ["warning", "caution", "attention"] },
  { title: "Falha", color: "hsl(358 85% 66%)", icon: "x", aliases: ["failure", "fail", "missing"] },
  { title: "Perigo", color: "hsl(358 85% 66%)", icon: "zap", aliases: ["danger", "error"] },
  { title: "Bug", color: "hsl(358 85% 66%)", icon: "bug", aliases: ["bug"] },
  { title: "Exemplo", color: "hsl(262 80% 74%)", icon: "list", aliases: ["example"] },
  { title: "Citação", color: "hsl(240 6% 62%)", icon: "quote", aliases: ["quote", "cite"] },
  // Tipos personalizados usados no cofre
  { title: "Link", color: "hsl(212 90% 66%)", icon: "link", aliases: ["link"] },
  { title: "Senha", color: "hsl(42 95% 60%)", icon: "key", aliases: ["password"] },
  { title: "Usuário", color: "hsl(262 80% 74%)", icon: "user", aliases: ["user"] },
  { title: "Código", color: "hsl(186 70% 56%)", icon: "code", aliases: ["code"] },
  { title: "Objetivo", color: "hsl(146 60% 52%)", icon: "target", aliases: ["objective", "goal"] },
  { title: "Rotina", color: "hsl(186 70% 56%)", icon: "repeat", aliases: ["routine"] },
  { title: "Cor", color: "hsl(320 75% 70%)", icon: "palette", aliases: ["color"] },
];

const BY_ALIAS = new Map(KINDS.flatMap((k) => k.aliases.map((a) => [a, k] as const)));

export function calloutKind(type: string): Kind {
  return (
    BY_ALIAS.get(type.toLowerCase()) ?? {
      title: type.charAt(0).toUpperCase() + type.slice(1).toLowerCase(),
      color: "var(--sx-accent)",
      icon: "pencil",
      aliases: [],
    }
  );
}

export const CALLOUT_HEADER = /^(\s*>\s*)\[!([\w-]+)\]([+-]?)\s?(.*)$/;

export const CALLOUT_TYPES = KINDS.slice(0, 13).map((k) => ({ type: k.aliases[0], title: k.title }));
