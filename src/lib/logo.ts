// Geometria da logo do Silex: pedra de sílex lapidada (minério facetado).
// Usada pelo componente Logo e pelo script que gera os ícones (scripts/make-icons.py
// lê os mesmos números). Coordenadas em 1024×1024; luz vindo do alto à esquerda.

/** Contorno externo e a face de cima ("mesa") da pedra. */
const OUTER = [
  [548, 104],
  [772, 330],
  [808, 612],
  [566, 920],
  [252, 726],
  [236, 372],
] as const;
const TABLE = [
  [516, 278],
  [664, 408],
  [668, 610],
  [538, 724],
  [384, 616],
  [378, 410],
] as const;
/** Opacidade de cada faceta lateral (entre OUTER[i], OUTER[i+1] e a mesa) e da mesa. */
const SHADE = [0.9, 0.68, 0.46, 0.38, 0.6, 0.82];
const TABLE_SHADE = 1;

const pts = (p: readonly (readonly [number, number])[]) => p.map(([x, y]) => `${x},${y}`).join(" ");

export const LOGO_FACETS: { points: string; opacity: number }[] = [
  ...OUTER.map((a, i) => {
    const b = OUTER[(i + 1) % 6];
    return { points: pts([a, b, TABLE[(i + 1) % 6], TABLE[i]]), opacity: SHADE[i] };
  }),
  { points: pts(TABLE), opacity: TABLE_SHADE },
];
