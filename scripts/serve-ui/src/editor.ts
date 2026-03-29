import { attachResizeHandles, applyStoredSize, addMaximizeToggle } from './resize.js';
import { ChunkEditor } from './cm.js';

const STORAGE_KEY = 'wb-editor';
const DEFAULT_W   = 640;

let panel!: HTMLElement;
let titleEl!: HTMLElement;
let statusEl!: HTMLElement;
let cm!: ChunkEditor;
let eFile = '', eName = '', eNth = 0, originalBody = '';

function setStatus(msg: string, kind: '' | 'ok' | 'error' = ''): void {
  statusEl.textContent = msg;
  statusEl.className   = kind;
}

async function loadChunk(file: string, name: string, nth: number, lang: string): Promise<void> {
  setStatus('Loading\u2026');
  try {
    const r = await fetch(`/__chunk?file=${encodeURIComponent(file)}&name=${encodeURIComponent(name)}&nth=${nth}`);
    const d = await r.json() as { ok: boolean; body?: string; error?: string };
    if (!d.ok) { setStatus(d.error ?? 'error', 'error'); return; }
    originalBody = d.body ?? '';
    cm.load(originalBody, lang);
    setStatus('');
  } catch (e) { setStatus(String(e), 'error'); }
}

async function saveChunk(): Promise<void> {
  setStatus('Saving\u2026');
  try {
    const newBody = cm.getValue();
    const r = await fetch('/__apply', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ file: eFile, name: eName, nth: eNth,
                             old_body: originalBody, new_body: newBody }),
    });
    const d = await r.json() as { ok: boolean; error?: string };
    if (!d.ok) { setStatus(d.error ?? 'error', 'error'); return; }
    setStatus('Saved \u2014 waiting for rebuild\u2026', 'ok');
    originalBody = newBody;
  } catch (e) { setStatus(String(e), 'error'); }
}

function openPanel(file: string, name: string, nth: number, lang: string): void {
  eFile = file; eName = name; eNth = nth;
  titleEl.textContent = `${name}  \u2014  ${file}`;
  titleEl.title       = `${file} | ${name} | nth=${nth}`;
  panel.classList.remove('hidden');
  void loadChunk(file, name, nth, lang);
}

function closePanel(): void {
  panel.classList.add('hidden');
  setStatus('');
  originalBody = '';
}

export function initEditor(): { open: (file: string, name: string, nth: number, lang: string) => void } {
  const defaultH = Math.round(window.innerHeight * 0.5);

  panel = document.createElement('div');
  panel.id = 'wb-editor-panel';
  panel.className = 'hidden';
  panel.innerHTML = `
    <div id="wb-editor-header">
      <span id="wb-editor-title"></span>
      <button class="wb-panel-btn" id="wb-editor-maximize" title="Maximize panel"></button>
      <button id="wb-editor-close" title="Close">\u00d7</button>
    </div>
    <div id="wb-editor-cm"></div>
    <div id="wb-editor-footer">
      <button class="wb-editor-btn primary" id="wb-editor-save">Save</button>
      <button class="wb-editor-btn" id="wb-editor-cancel">Cancel</button>
      <span id="wb-editor-status"></span>
      <span class="wb-kbd-hint">Ctrl+S\u00a0save\u2003Esc\u00a0close\u2003Tab\u00a0indent\u2003drag\u00a0edges\u00a0to\u00a0resize</span>
    </div>`;
  document.body.appendChild(panel);

  titleEl  = document.getElementById('wb-editor-title')!;
  statusEl = document.getElementById('wb-editor-status')!;

  cm = new ChunkEditor(
    document.getElementById('wb-editor-cm')!,
    () => { void saveChunk(); },
    closePanel,
  );

  applyStoredSize(panel, STORAGE_KEY, DEFAULT_W, defaultH);
  attachResizeHandles({ el: panel, storageKey: STORAGE_KEY, anchor: 'bottom-right', defaultW: DEFAULT_W, defaultH });
  addMaximizeToggle(
    document.getElementById('wb-editor-maximize') as HTMLButtonElement,
    panel, STORAGE_KEY, DEFAULT_W, defaultH,
  );

  document.getElementById('wb-editor-close')!.addEventListener('click', closePanel);
  document.getElementById('wb-editor-cancel')!.addEventListener('click', closePanel);
  document.getElementById('wb-editor-save')!.addEventListener('click', () => { void saveChunk(); });

  return { open: openPanel };
}
