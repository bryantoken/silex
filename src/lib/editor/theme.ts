import { HighlightStyle } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

// Cores vêm dos tokens --sx-* definidos em src/styles/silex.css
export const silexHighlight = HighlightStyle.define([
  { tag: t.heading1, fontSize: "1.9em", fontWeight: "700", letterSpacing: "-0.02em" },
  { tag: t.heading2, fontSize: "1.55em", fontWeight: "700", letterSpacing: "-0.015em" },
  { tag: t.heading3, fontSize: "1.3em", fontWeight: "650" },
  { tag: t.heading4, fontSize: "1.12em", fontWeight: "650" },
  { tag: [t.heading5, t.heading6], fontSize: "1em", fontWeight: "650", color: "var(--sx-text-muted)" },
  { tag: t.strong, fontWeight: "700", color: "var(--sx-text-strong)" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through", color: "var(--sx-text-muted)" },
  { tag: t.link, color: "var(--sx-accent-text)" },
  { tag: t.url, color: "var(--sx-text-faint)" },
  { tag: t.monospace, fontFamily: "var(--sx-font-mono)", fontSize: "0.88em" },
  { tag: [t.processingInstruction, t.meta], color: "var(--sx-text-faint)" },
  { tag: t.contentSeparator, color: "var(--sx-text-faint)" },
  // Realce dentro de blocos de código
  { tag: [t.keyword, t.modifier, t.operatorKeyword], color: "var(--sx-syntax-keyword)" },
  { tag: [t.string, t.special(t.string), t.regexp], color: "var(--sx-syntax-string)" },
  { tag: [t.number, t.bool, t.null, t.atom], color: "var(--sx-syntax-number)" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--sx-syntax-function)" },
  { tag: [t.typeName, t.className, t.namespace], color: "var(--sx-syntax-type)" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--sx-text-faint)", fontStyle: "italic" },
  { tag: [t.propertyName, t.attributeName], color: "var(--sx-syntax-property)" },
]);

export const silexEditorTheme = EditorView.theme(
  {
    "&": {
      color: "var(--sx-text)",
      backgroundColor: "transparent",
      fontSize: "var(--sx-editor-size)",
      height: "100%",
    },
    "&.cm-focused": { outline: "none" },
    // Recuo lateral fica no scroller, não no conteúdo: o CodeMirror desenha a
    // seleção até a borda do conteúdo, e o padding ali fazia a seleção vazar do texto
    ".cm-scroller": {
      fontFamily: "var(--sx-font-text)",
      lineHeight: "1.7",
      overflow: "auto",
      padding: "0 40px",
    },
    ".cm-content": {
      maxWidth: "var(--sx-line-width)",
      margin: "0 auto",
      padding: "8px 0 40vh",
      caretColor: "var(--sx-accent)",
    },
    ".cm-line": { padding: "0" },
    ".cm-cursor, .cm-dropCursor": { borderLeft: "2px solid var(--sx-accent)" },
    // Só a camada de seleção do CodeMirror pinta a seleção (a nativa fica oculta pelo drawSelection)
    ".cm-selectionBackground": { backgroundColor: "var(--sx-selection-dim)" },
    "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
      backgroundColor: "var(--sx-selection)",
    },
    ".cm-selectionMatch": { backgroundColor: "var(--sx-accent-soft)" },
    ".cm-matchingBracket": { backgroundColor: "var(--sx-accent-soft)", outline: "none" },
    ".cm-placeholder": { color: "var(--sx-text-faint)" },

    // Painel de busca/substituição (Ctrl+F)
    ".cm-panels": {
      backgroundColor: "var(--sx-surface-raised)",
      color: "var(--sx-text)",
      borderColor: "var(--sx-border)",
    },
    ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--sx-border)" },
    ".cm-panel.cm-search": { padding: "8px 12px", fontFamily: "var(--sx-font-ui)", fontSize: "var(--sx-text-sm)" },
    ".cm-panel.cm-search input, .cm-panel.cm-search button": {
      fontFamily: "inherit",
      fontSize: "inherit",
      borderRadius: "var(--sx-radius-sm)",
    },
    ".cm-textfield": {
      backgroundColor: "var(--sx-surface-sunken)",
      border: "1px solid var(--sx-border)",
      color: "var(--sx-text)",
    },
    ".cm-button": {
      backgroundImage: "none",
      backgroundColor: "var(--sx-surface-hover)",
      border: "1px solid var(--sx-border)",
      color: "var(--sx-text)",
    },
    // Sugestões de [[links]]
    ".cm-tooltip": {
      backgroundColor: "var(--sx-surface-raised)",
      border: "1px solid var(--sx-border-strong)",
      borderRadius: "var(--sx-radius-md)",
      boxShadow: "var(--sx-shadow-pop)",
      color: "var(--sx-text)",
      overflow: "hidden",
    },
    ".cm-tooltip.cm-tooltip-autocomplete > ul": {
      fontFamily: "var(--sx-font-ui)",
      fontSize: "var(--sx-text-md)",
      maxHeight: "18em",
      minWidth: "260px",
      maxWidth: "min(520px, 80vw)",
      padding: "4px",
    },
    ".cm-tooltip.cm-tooltip-autocomplete > ul > li": {
      display: "flex",
      alignItems: "baseline",
      gap: "10px",
      padding: "4px 8px",
      borderRadius: "var(--sx-radius-sm)",
      lineHeight: "1.5",
    },
    ".cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]": {
      backgroundColor: "var(--sx-accent-soft-strong)",
      color: "var(--sx-text-strong)",
    },
    ".cm-completionLabel": { whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" },
    ".cm-completionMatchedText": { textDecoration: "none", color: "var(--sx-accent-text)", fontWeight: "600" },
    ".cm-completionDetail": {
      marginLeft: "auto",
      fontStyle: "normal",
      fontSize: "var(--sx-text-xs)",
      color: "var(--sx-text-faint)",
      whiteSpace: "nowrap",
      overflow: "hidden",
      textOverflow: "ellipsis",
    },
    ".cm-searchMatch": { backgroundColor: "var(--sx-mark)", borderRadius: "2px" },
    ".cm-searchMatch-selected": { backgroundColor: "var(--sx-accent-soft-strong)" },
  },
  { dark: true },
);
