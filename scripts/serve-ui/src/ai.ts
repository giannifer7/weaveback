import { attachResizeHandles, applyStoredSize, addMaximizeToggle } from './resize.js';

const STORAGE_KEY = 'wb-ai';
const DEFAULT_W   = 600;

interface SseEvent { type: string; data: string; }

function parseSse(buf: string): { events: SseEvent[]; remaining: string } {
  const events: SseEvent[] = [];
  let remaining = buf;
  while (true) {
    const idx = remaining.indexOf('\n\n');
    if (idx === -1) break;
    const block = remaining.slice(0, idx);
    remaining = remaining.slice(idx + 2);
    let type = 'message', data = '';
    for (const line of block.split('\n')) {
      if (line.startsWith('event: ')) type = line.slice(7).trim();
      else if (line.startsWith('data: ')) data = line.slice(6);
    }
    events.push({ type, data });
  }
  return { events, remaining };
}

let panel!: HTMLElement;
let aiTitleEl!: HTMLElement;
let messagesEl!: HTMLElement;
let inputEl!: HTMLTextAreaElement;
let sendBtn!: HTMLButtonElement;
let aFile = '', aName = '', aNth = 0;

function appendMsg(role: string, text: string): HTMLElement {
  const el = document.createElement('div');
  el.className = `wb-ai-msg ${role}`;
  el.textContent = text;
  messagesEl.appendChild(el);
  messagesEl.scrollTop = messagesEl.scrollHeight;
  return el;
}

function attachSaveBtn(el: HTMLElement, text: string, file: string, name: string, nth: number): void {
  const btn = document.createElement('button');
  btn.className = 'wb-ai-save-btn';
  btn.textContent = '\u2193 Save as [NOTE]';
  btn.addEventListener('click', async () => {
    btn.disabled = true; btn.textContent = 'Saving\u2026';
    try {
      const r = await fetch('/__save_note', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ file, name, nth, note: text }),
      });
      const d = await r.json() as { ok: boolean; error?: string };
      btn.textContent = d.ok ? '\u2713 Saved' : `\u2717 ${d.error ?? 'failed'}`;
    } catch (e) { btn.textContent = `\u2717 ${String(e)}`; }
  });
  el.append(document.createElement('br'), btn);
}

async function sendQuestion(): Promise<void> {
  const question = inputEl.value.trim();
  if (!question) return;
  inputEl.value = '';
  sendBtn.disabled = true;
  appendMsg('user', question);
  const assistantEl = appendMsg('assistant', '\u2026');
  let fullText = '';

  const body: Record<string, unknown> = { question };
  if (aFile) { body.file = aFile; body.name = aName; body.nth = aNth; }

  try {
    const resp = await fetch('/__ai', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });

    if ((resp.headers.get('content-type') ?? '').includes('application/json')) {
      const d = await resp.json() as { error?: string };
      assistantEl.textContent = d.error ?? 'Error';
      assistantEl.classList.add('system');
    } else {
      const reader  = resp.body!.getReader();
      const decoder = new TextDecoder();
      let buf = '', done = false;

      while (!done) {
        const { value, done: rdDone } = await reader.read();
        if (rdDone) break;
        buf += decoder.decode(value, { stream: true });
        const parsed = parseSse(buf);
        buf = parsed.remaining;
        for (const ev of parsed.events) {
          switch (ev.type) {
            case 'token':
              try { fullText += (JSON.parse(ev.data) as { t?: string }).t ?? ''; } catch { /* skip */ }
              assistantEl.textContent = fullText;
              messagesEl.scrollTop = messagesEl.scrollHeight;
              break;
            case 'error':
              try { assistantEl.textContent = (JSON.parse(ev.data) as { error?: string }).error ?? 'Error'; }
              catch { assistantEl.textContent = 'Error'; }
              assistantEl.classList.add('system');
              done = true; break;
            case 'done':
              if (!fullText) assistantEl.textContent = '(no response)';
              if (fullText && aFile) attachSaveBtn(assistantEl, fullText, aFile, aName, aNth);
              done = true; break;
          }
        }
      }
    }
  } catch (e) {
    assistantEl.textContent = String(e);
    assistantEl.classList.add('system');
  } finally {
    sendBtn.disabled = false;
  }
}

export function openAiPanel(file?: string, name?: string, nth?: number): void {
  aFile = file ?? ''; aName = name ?? ''; aNth = nth ?? 0;
  aiTitleEl.textContent = name ? `${name}  \u2014  ${file}` : 'AI Assistant';
  panel.classList.remove('hidden');
  inputEl.focus();
  if (name) appendMsg('system', `Context: chunk \u201c${name}\u201d from ${file}`);
}

function closeAiPanel(): void { panel.classList.add('hidden'); }

export function initAi(): { open: typeof openAiPanel } {
  const defaultH = Math.round(window.innerHeight * 0.5);

  panel = document.createElement('div');
  panel.id = 'wb-ai-panel';
  panel.className = 'hidden';
  panel.innerHTML = `
    <div id="wb-ai-header">
      <span id="wb-ai-title">AI Assistant</span>
      <button class="wb-panel-btn" id="wb-ai-maximize" title="Maximize panel"></button>
      <button id="wb-ai-close" title="Close">\u00d7</button>
    </div>
    <div id="wb-ai-messages"></div>
    <div id="wb-ai-actions">
      <button class="wb-ai-action" data-prompt="Explain what this chunk does and why it\u2019s designed this way.">Explain</button>
      <button class="wb-ai-action" data-prompt="Explain the direct dependencies of this chunk and how they are used.">Deps</button>
      <button class="wb-ai-action" data-prompt="Where is this chunk referenced, and what does it contribute to the output?">Where used?</button>
      <button class="wb-ai-action" data-prompt="What could be improved in this chunk? Consider correctness, clarity, and edge cases.">Improve</button>
    </div>
    <div id="wb-ai-input-row">
      <textarea id="wb-ai-input" rows="2" placeholder="Ask about this chunk\u2026" spellcheck="false"></textarea>
      <button id="wb-ai-send">Send</button>
    </div>
    <div class="wb-kbd-hint" style="padding:2px 8px 4px">Enter\u00a0send\u2003Shift+Enter\u00a0newline\u2003Esc\u00a0close\u2003drag\u00a0edges\u00a0to\u00a0resize</div>`;
  document.body.appendChild(panel);

  const toggleBtn = document.createElement('button');
  toggleBtn.id = 'wb-ai-toggle';
  toggleBtn.textContent = '\u2736 AI';
  toggleBtn.title = 'Open AI assistant';
  document.body.appendChild(toggleBtn);

  aiTitleEl  = document.getElementById('wb-ai-title')!;
  messagesEl = document.getElementById('wb-ai-messages')!;
  inputEl    = document.getElementById('wb-ai-input') as HTMLTextAreaElement;
  sendBtn    = document.getElementById('wb-ai-send') as HTMLButtonElement;

  applyStoredSize(panel, STORAGE_KEY, DEFAULT_W, defaultH);
  attachResizeHandles({ el: panel, storageKey: STORAGE_KEY, anchor: 'bottom-left', defaultW: DEFAULT_W, defaultH });
  addMaximizeToggle(
    document.getElementById('wb-ai-maximize') as HTMLButtonElement,
    panel, STORAGE_KEY, DEFAULT_W, defaultH,
  );

  document.getElementById('wb-ai-close')!.addEventListener('click', closeAiPanel);
  toggleBtn.addEventListener('click', () => {
    panel.classList.contains('hidden') ? openAiPanel() : closeAiPanel();
  });
  sendBtn.addEventListener('click', () => { void sendQuestion(); });
  inputEl.addEventListener('keydown', e => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void sendQuestion(); }
    if (e.key === 'Escape') closeAiPanel();
  });
  panel.querySelectorAll<HTMLButtonElement>('.wb-ai-action').forEach(btn => {
    btn.addEventListener('click', () => { inputEl.value = btn.dataset.prompt ?? ''; void sendQuestion(); });
  });

  return { open: openAiPanel };
}
