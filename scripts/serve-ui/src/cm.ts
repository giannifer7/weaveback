import { Compartment, EditorState } from '@codemirror/state';
import {
  EditorView, keymap, lineNumbers,
  highlightActiveLine, highlightActiveLineGutter,
  drawSelection,
} from '@codemirror/view';
import { defaultKeymap, historyKeymap, history, indentWithTab } from '@codemirror/commands';
import {
  bracketMatching, indentOnInput,
  HighlightStyle, syntaxHighlighting, foldGutter,
} from '@codemirror/language';
import { rust } from '@codemirror/lang-rust';
import { javascript } from '@codemirror/lang-javascript';
import { python } from '@codemirror/lang-python';
import { tags as t } from '@lezer/highlight';

// ── Gruvbox Dark theme ────────────────────────────────────────────
const theme = EditorView.theme({
  '&': {
    height: '100%',
    backgroundColor: '#1d2021',
    color: '#ebdbb2',
  },
  '.cm-scroller': { overflow: 'auto', fontFamily: '"SFMono-Regular", Consolas, monospace', fontSize: '.82rem' },
  '.cm-content': { caretColor: '#ebdbb2', padding: '.4em 0' },
  '.cm-cursor': { borderLeftColor: '#ebdbb2' },
  '.cm-activeLine': { backgroundColor: '#282828' },
  '.cm-gutters': {
    backgroundColor: '#1d2021',
    color: '#928374',
    border: 'none',
    borderRight: '1px solid #3c3836',
  },
  '.cm-activeLineGutter': { backgroundColor: '#282828' },
  '.cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection': {
    backgroundColor: '#504945 !important',
  },
  '.cm-matchingBracket': { color: '#b8bb26 !important', fontWeight: 'bold' },
  '.cm-foldPlaceholder': { backgroundColor: '#3c3836', border: 'none', color: '#928374' },
  '.cm-foldGutter span': { color: '#504945' },
  '.cm-foldGutter span:hover': { color: '#fabd2f' },
}, { dark: true });

const highlighting = HighlightStyle.define([
  { tag: t.keyword,                        color: '#fb4934' },
  { tag: t.controlKeyword,                 color: '#fb4934' },
  { tag: t.operator,                       color: '#fe8019' },
  { tag: t.operatorKeyword,                color: '#fe8019' },
  { tag: t.string,                         color: '#b8bb26' },
  { tag: t.special(t.string),              color: '#b8bb26' },
  { tag: t.number,                         color: '#d3869b' },
  { tag: t.integer,                        color: '#d3869b' },
  { tag: t.float,                          color: '#d3869b' },
  { tag: t.bool,                           color: '#d3869b' },
  { tag: t.null,                           color: '#d3869b' },
  // Line comments green — weaveback chunk refs live here
  { tag: t.lineComment,                    color: '#b8bb26' },
  { tag: t.blockComment,                   color: '#928374', fontStyle: 'italic' },
  { tag: t.docComment,                     color: '#928374', fontStyle: 'italic' },
  { tag: t.function(t.variableName),       color: '#fabd2f' },
  { tag: t.function(t.propertyName),       color: '#fabd2f' },
  { tag: t.definition(t.variableName),     color: '#83a598' },
  { tag: t.definition(t.function(t.variableName)), color: '#fabd2f' },
  { tag: t.typeName,                       color: '#fabd2f' },
  { tag: t.className,                      color: '#fabd2f' },
  { tag: t.namespace,                      color: '#83a598' },
  { tag: t.macroName,                      color: '#fe8019' },
  { tag: t.propertyName,                   color: '#8ec07c' },
  { tag: t.attributeName,                  color: '#8ec07c' },
  { tag: t.self,                           color: '#fb4934', fontStyle: 'italic' },
  { tag: t.variableName,                   color: '#ebdbb2' },
  { tag: t.punctuation,                    color: '#ebdbb2' },
  { tag: t.angleBracket,                   color: '#ebdbb2' },
  { tag: t.bracket,                        color: '#ebdbb2' },
  { tag: t.escape,                         color: '#fe8019' },
]);

// ── Language detection ────────────────────────────────────────────
// lang is the value of data-lang on the <code> element set by acdc,
// e.g. "rust", "typescript", "javascript", "python".
function detectLang(lang: string) {
  switch (lang.toLowerCase()) {
    case 'rust':        return rust();
    case 'typescript':
    case 'ts':          return javascript({ typescript: true });
    case 'tsx':         return javascript({ typescript: true, jsx: true });
    case 'javascript':
    case 'js':
    case 'mjs':         return javascript();
    case 'jsx':         return javascript({ jsx: true });
    case 'python':
    case 'py':          return python();
    default:            return null;
  }
}

// ── ChunkEditor ───────────────────────────────────────────────────
export class ChunkEditor {
  private view: EditorView;
  private lang = new Compartment();

  constructor(parent: HTMLElement, onSave: () => void, onClose: () => void) {
    this.view = new EditorView({
      state: EditorState.create({
        extensions: [
          lineNumbers(),
          foldGutter(),
          history(),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          drawSelection(),
          bracketMatching(),
          indentOnInput(),
          keymap.of([
            { key: 'Ctrl-s', mac: 'Cmd-s', run: () => { onSave(); return true; } },
            { key: 'Escape', run: () => { onClose(); return true; } },
            indentWithTab,
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          theme,
          syntaxHighlighting(highlighting),
          this.lang.of([]),
        ],
      }),
      parent,
    });
  }

  load(content: string, lang: string): void {
    const langExt = detectLang(lang);
    this.view.dispatch({
      changes: { from: 0, to: this.view.state.doc.length, insert: content },
      effects: this.lang.reconfigure(langExt ?? []),
      selection: { anchor: 0 },
      scrollIntoView: true,
    });
    this.view.focus();
  }

  getValue(): string {
    return this.view.state.doc.toString();
  }
}
