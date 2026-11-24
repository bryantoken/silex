/*
 * Listas suspensas (<select>) no tema do Silex.
 *
 * O WebView desenha a lista aberta de um <select> com o visual do sistema, e o CSS
 * não alcança essa parte. Aqui o clique (ou Espaço/Enter/Alt+↓) num <select> é
 * interceptado e a lista abre como um menu do Silex (lib/menu.ts), ancorado no campo.
 * Vale para todos os <select> do app e dos plugins (Mesa, Chat, configurações...),
 * inclusive os criados depois, sem trocar nenhum componente: a escolha muda o
 * `value` do próprio <select> e dispara input/change como o nativo.
 */
import { closeMenu, menuOpen, popupMenu, type MenuEntry } from "./menu";

let owner: HTMLSelectElement | null = null;
let reopenGuard: HTMLSelectElement | null = null;

const isDropdown = (s: HTMLSelectElement) => !s.multiple && s.size <= 1;

function choose(sel: HTMLSelectElement, option: HTMLOptionElement) {
  sel.focus();
  if (option.selected) return;
  option.selected = true;
  sel.dispatchEvent(new Event("input", { bubbles: true }));
  sel.dispatchEvent(new Event("change", { bubbles: true }));
}

function entry(sel: HTMLSelectElement, o: HTMLOptionElement): MenuEntry | null {
  if (o.hidden) return null;
  return { text: o.label || o.text || " ", checked: o.selected, enabled: !o.disabled, action: () => choose(sel, o) };
}

export function openSelect(sel: HTMLSelectElement) {
  const entries: MenuEntry[] = [];
  for (const child of Array.from(sel.children)) {
    if (child instanceof HTMLOptGroupElement) {
      if (entries.length) entries.push("separator");
      if (child.label) entries.push({ text: child.label, action: () => {}, enabled: false });
      for (const o of Array.from(child.children)) {
        const e = o instanceof HTMLOptionElement ? entry(sel, o) : null;
        if (e) entries.push(e);
      }
    } else if (child instanceof HTMLOptionElement) {
      const e = entry(sel, child);
      if (e) entries.push(e);
    }
  }
  if (!entries.length) return;
  const r = sel.getBoundingClientRect();
  owner = sel;
  sel.classList.add("is-open");
  popupMenu(entries, {
    x: r.left,
    y: r.bottom + 4,
    flipY: r.top - 4,
    minWidth: r.width,
    focusChecked: true,
    typeahead: true,
    className: "is-select",
  });
  // Marca o campo como aberto enquanto a lista estiver na tela
  const watch = () => {
    if (menuOpen() && owner === sel) return requestAnimationFrame(watch);
    sel.classList.remove("is-open");
    if (owner === sel) owner = null;
  };
  requestAnimationFrame(watch);
}

let installed = false;

export function installSelects() {
  if (installed || typeof window === "undefined") return;
  installed = true;

  // pointerdown vem antes do "clique fora" do menu: lembra se a lista deste campo
  // já estava aberta, para o clique no campo fechar em vez de reabrir
  window.addEventListener(
    "pointerdown",
    (e) => {
      const sel = (e.target as Element | null)?.closest?.("select");
      reopenGuard = sel && owner === sel && menuOpen() ? sel : null;
    },
    true,
  );

  window.addEventListener(
    "mousedown",
    (e) => {
      const sel = (e.target as Element | null)?.closest?.("select");
      if (!sel || !isDropdown(sel) || sel.disabled || e.button !== 0) return;
      e.preventDefault(); // impede a lista nativa
      sel.focus();
      if (reopenGuard === sel) {
        reopenGuard = null;
        closeMenu();
        return;
      }
      openSelect(sel);
    },
    true,
  );

  window.addEventListener(
    "keydown",
    (e) => {
      const sel = e.target;
      if (!(sel instanceof HTMLSelectElement) || !isDropdown(sel) || sel.disabled || menuOpen()) return;
      const open = e.key === " " || e.key === "Enter" || e.key === "F4" || (e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp"));
      if (!open) return;
      e.preventDefault();
      e.stopPropagation();
      openSelect(sel);
    },
    true,
  );
}
