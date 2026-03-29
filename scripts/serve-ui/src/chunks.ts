type OpenEditor = (file: string, name: string, nth: number, lang: string) => void;
type OpenAi     = (file?: string, name?: string, nth?: number) => void;

export function attachChunkButtons(openEditor: OpenEditor, openAi: OpenAi): void {
  document.querySelectorAll<HTMLElement>('.listingblock[data-chunk-id]').forEach(block => {
    const parts = (block.dataset.chunkId ?? '').split('|');
    if (parts.length < 3) return;
    const [file, name, nthStr] = parts;
    const nth = parseInt(nthStr, 10);
    // asciidoctor/rouge sets data-lang="rust" (etc.) on the <code> element
    const lang = block.querySelector<HTMLElement>('code[data-lang]')?.dataset.lang ?? '';

    const editBtn = document.createElement('button');
    editBtn.className = 'wb-chunk-btn';
    editBtn.textContent = '\u270e Edit';
    editBtn.title = `Edit chunk \u201c${name}\u201d`;
    editBtn.addEventListener('click', e => { e.stopPropagation(); openEditor(file, name, nth, lang); });
    block.appendChild(editBtn);

    const aiBtn = document.createElement('button');
    aiBtn.className = 'wb-ai-btn';
    aiBtn.textContent = '\u2736 Ask';
    aiBtn.title = `Ask AI about chunk \u201c${name}\u201d`;
    aiBtn.addEventListener('click', e => { e.stopPropagation(); openAi(file, name, nth); });
    block.appendChild(aiBtn);
  });
}
