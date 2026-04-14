import { initLiveReload, initEditButton } from './live.js';
import { initToc } from './toc.js';
import { initXref } from './xref.js';
import { initEditor } from './editor.js';
import { initAi } from './ai.js';
import { attachChunkButtons } from './chunks.js';

// Start SSE live reload immediately — no DOM needed
initLiveReload();

document.addEventListener('DOMContentLoaded', () => {
  initToc();
  initXref();
  initEditButton();

  if (window.location.protocol === 'file:') return;

  const isLocal = ['localhost', '127.0.0.1', '::1'].includes(window.location.hostname);

  if (!isLocal) {
    const hint = document.createElement('div');
    hint.id = 'wb-serve-hint';
    hint.innerHTML =
      '\u2736 <a href="https://github.com/giannifer7/weaveback#live-documentation-server"'
      + ' target="_blank" rel="noopener">Run <code>wb-serve</code> locally</a>'
      + ' for the inline editor and AI assistant.';
    document.body.appendChild(hint);
    return;
  }

  document.body.classList.add('wb-local');

  const { open: openEditor } = initEditor();
  const { open: openAi }     = initAi();
  attachChunkButtons(openEditor, openAi);
});
