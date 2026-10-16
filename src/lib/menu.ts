/*
 * Menus de contexto do app, no tema do Silex (HTML), no lugar dos nativos do SO.
 *
 * Mais leve que o menu nativo: cada menu nativo criava recursos no Rust por IPC
 * (um por item) a cada clique; aqui são alguns elementos DOM, criados ao abrir e
 * descartados ao fechar. Abre na posição do último clique.
 */
import { EditorView } from "@codemirror/view";
import { svgIcon } from "./icons";

export type MenuEntry =
  | { text: string; action: () => void; enabled?: boolean; checked?: boolean }
  | { text: string; items: MenuEntry[] }
  | "separator"
  | "cut"
  | "copy"
  | "paste"
  | "selectAll";

type Clip = "cut" | "copy" | "paste" | "selectAll";
const CLIP: Record<Clip, { text: string; hint: string }> = {
  cut: { text: "Recortar", hint: "Ctrl+X" },
  copy: { text: "Copiar", hint: "Ctrl+C" },
  paste: { text: "Colar", hint: "Ctrl+V" },
  selectAll: { text: "Selecionar tudo", hint: "Ctrl+A" },
};

let pointer = { x: 0, y: 0 };
if (typeof window !== "undefined") {
  const track = (e: MouseEvent) => (pointer = { x: e.clientX, y: e.clientY });
  window.addEventListener("pointerdown", track, true);
  window.addEventListener("contextmenu", track, true);
}

let close: (() => void) | null = null;

export function closeMenu() {
  close?.();
}

// ---------- Área de transferência (itens Recortar/Copiar/Colar/Selecionar tudo) ----------

function editorOf(el: Element | null) {
  const root = el?.closest(".cm-editor");
  return root ? EditorView.findFromDOM(root as HTMLElement) : null;
}

async function clipboard(kind: Clip, target: HTMLElement | null) {
  target?.focus();
  const view = editorOf(target);
  if (kind === "cut" || kind === "copy") {
    document.execCommand(kind);
    return;
  }
  if (kind === "selectAll") {
    if (view) view.dispatch({ selection: { anchor: 0, head: view.state.doc.length } });
    else if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) target.select();
    else document.execCommand("selectAll");
    return;
  }
  // Colar: monta um evento de colar com o conteúdo da área de transferência,
  // para passar pelos mesmos tratamentos do Ctrl+V (Excel → tabela, imagem → anexo)
  const data = new DataTransfer();
  try {
    for (const item of await navigator.clipboard.read()) {
      for (const type of item.types) {
        const blob = await item.getType(type);
        if (type.startsWith("image/")) data.items.add(new File([blob], `imagem.${type.split("/")[1]}`, { type }));
        else if (type === "text/plain" || type === "text/html") data.setData(type, await blob.text());
      }
    }
  } catch {
    data.setData("text/plain", await navigator.clipboard.readText().catch(() => ""));
  }
  const text = data.getData("text/plain");
  if (view) {
    const ev = new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true });
    view.contentDOM.dispatchEvent(ev);
    if (!ev.defaultPrevented && text) view.dispatch(view.state.replaceSelection(text));
  } else if (text) {
    document.execCommand("insertText", false, text);
  }
}

// ---------- Menu ----------

type Panel = { el: HTMLElement; items: HTMLButtonElement[]; active: number; child: Panel | null; parent: Panel | null };

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, text?: string) {
  const e = document.createElement(tag);
  e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

/** Abre o menu na posição do último clique (ou em `at`; com `above`, o menu termina em `y`). */
export function popupMenu(entries: MenuEntry[], at: { x: number; y: number; above?: boolean } = pointer): Promise<void> {
  closeMenu();
  const target = document.activeElement as HTMLElement | null;
  const layer = el("div", "sx-menu-layer");
  document.body.append(layer);

  const finish = () => {
    layer.remove();
    window.removeEventListener("pointerdown", outside, true);
    window.removeEventListener("keydown", keys, true);
    window.removeEventListener("blur", finish);
    window.removeEventListener("resize", finish);
    window.removeEventListener("wheel", wheel, true);
    if (close === finish) close = null;
  };
  close = finish;

  const run = (fn: () => void) => {
    finish();
    fn();
  };

  const setActive = (p: Panel, i: number) => {
    p.items[p.active]?.classList.remove("is-active");
    p.active = i;
    p.items[i]?.classList.add("is-active");
  };

  const closeChild = (p: Panel) => {
    if (!p.child) return;
    closeChild(p.child);
    p.child.el.remove();
    p.child = null;
  };

  const place = (panel: HTMLElement, x: number, y: number, alt?: number, above = false) => {
    const w = panel.offsetWidth;
    const h = panel.offsetHeight;
    if (above) y = Math.max(4, y - h);
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    // Sem espaço à direita, abre para a esquerda (submenu: do outro lado do pai)
    const left = x + w > vw - 4 ? Math.max(4, (alt ?? x) - w) : x;
    const top = y + h > vh - 4 ? Math.max(4, vh - h - 4) : y;
    panel.style.left = `${left}px`;
    panel.style.top = `${top}px`;
  };

  const build = (list: MenuEntry[], parent: Panel | null): Panel => {
    const box = el("div", "sx-menu");
    box.setAttribute("role", "menu");
    const panel: Panel = { el: box, items: [], active: -1, child: null, parent };
    for (const entry of list) {
      if (!entry) continue;
      if (entry === "separator") {
        box.append(el("div", "sx-menu-sep"));
        continue;
      }
      const btn = el("button", "sx-menu-item");
      btn.setAttribute("role", "menuitem");
      const check = el("span", "sx-menu-check");
      const label = el("span", "sx-menu-label");
      btn.append(check, label);
      let activate: () => void;
      if (typeof entry === "string") {
        label.textContent = CLIP[entry].text;
        btn.append(el("span", "sx-menu-hint", CLIP[entry].hint));
        activate = () => run(() => void clipboard(entry, target));
      } else if ("items" in entry) {
        label.textContent = entry.text;
        btn.append(svgIcon("chevron", 13, 2));
        btn.classList.add("has-sub");
        const open = () => {
          if (panel.child?.el.dataset.owner === entry.text) return;
          closeChild(panel);
          const child = build(entry.items, panel);
          child.el.dataset.owner = entry.text;
          layer.append(child.el);
          const r = btn.getBoundingClientRect();
          place(child.el, r.right + 2, r.top - 5, r.left - 2);
          panel.child = child;
        };
        btn.addEventListener("pointerenter", open);
        activate = () => {
          open();
          if (panel.child) setActive(panel.child, 0);
        };
      } else {
        label.textContent = entry.text;
        if (entry.checked) check.append(svgIcon("check", 13, 2.4));
        btn.disabled = entry.enabled === false;
        activate = () => run(entry.action);
      }
      const index = panel.items.length;
      btn.addEventListener("pointerenter", () => {
        setActive(panel, index);
        if (!btn.classList.contains("has-sub")) closeChild(panel);
      });
      btn.addEventListener("click", () => !btn.disabled && activate());
      (btn as HTMLButtonElement & { _activate?: () => void })._activate = activate;
      panel.items.push(btn);
      box.append(btn);
    }
    // Não tira o foco do editor/campo (a seleção continua lá para copiar/colar)
    box.addEventListener("mousedown", (e) => e.preventDefault());
    box.addEventListener("contextmenu", (e) => e.preventDefault());
    return panel;
  };

  const root = build(entries, null);
  layer.append(root.el);
  place(root.el, at.x, at.y, undefined, at.above);

  const deepest = () => {
    let p = root;
    while (p.child && p.child.active >= 0) p = p.child;
    return p;
  };
  const step = (p: Panel, dir: 1 | -1) => {
    const n = p.items.length;
    for (let k = 1; k <= n; k++) {
      const i = (p.active + dir * k + n * 2) % n;
      if (!p.items[i].disabled) return setActive(p, i);
    }
  };

  function outside(e: PointerEvent) {
    if (!layer.contains(e.target as Node)) finish();
  }
  function wheel(e: WheelEvent) {
    if (!layer.contains(e.target as Node)) finish();
  }
  function keys(e: KeyboardEvent) {
    const p = deepest();
    const handled = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    if (e.key === "Escape") {
      handled();
      if (p.parent) {
        closeChild(p.parent);
      } else finish();
    } else if (e.key === "ArrowDown") {
      handled();
      step(p, 1);
    } else if (e.key === "ArrowUp") {
      handled();
      step(p, -1);
    } else if (e.key === "ArrowRight") {
      handled();
      const btn = p.items[p.active] as (HTMLButtonElement & { _activate?: () => void }) | undefined;
      if (btn?.classList.contains("has-sub")) btn._activate?.();
    } else if (e.key === "ArrowLeft") {
      handled();
      if (p.parent) closeChild(p.parent);
    } else if (e.key === "Enter" || e.key === " ") {
      handled();
      const btn = p.items[p.active] as (HTMLButtonElement & { _activate?: () => void }) | undefined;
      if (btn && !btn.disabled) btn._activate?.();
    } else if (e.key !== "Shift" && e.key !== "Control" && e.key !== "Alt") {
      finish();
    }
  }

  // Registra depois do evento atual, para o clique que abriu o menu não fechá-lo
  setTimeout(() => {
    if (close !== finish) return;
    window.addEventListener("pointerdown", outside, true);
    window.addEventListener("keydown", keys, true);
    window.addEventListener("blur", finish);
    window.addEventListener("resize", finish);
    window.addEventListener("wheel", wheel, true);
  }, 0);
  return Promise.resolve();
}
