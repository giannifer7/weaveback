interface XrefLink { html: string; label: string; key: string; }

interface XrefData {
  self?: string;
  imports?: XrefLink[];
  importedBy?: XrefLink[];
  symbols?: string[];
}

declare global {
  interface Window { __xref?: XrefData; }
}

function relPathTo(selfHtml: string, targetHtml: string): string {
  const selfParts = selfHtml.split('/');
  selfParts.pop();
  const toParts = targetHtml.split('/');
  let i = 0;
  while (i < selfParts.length && i < toParts.length && selfParts[i] === toParts[i]) i++;
  return [...selfParts.slice(i).map(() => '..'), ...toParts.slice(i)].join('/');
}

function linkList(links: XrefLink[], selfHtml: string): HTMLElement {
  const ul = document.createElement('ul');
  for (const l of links) {
    const li = document.createElement('li');
    const a = document.createElement('a');
    a.href = relPathTo(selfHtml, l.html);
    a.textContent = l.label;
    a.title = l.key;
    li.appendChild(a);
    ul.appendChild(li);
  }
  return ul;
}

function section(title: string, content: HTMLElement): HTMLElement {
  const div = document.createElement('div');
  div.className = 'xref-section';
  const h3 = document.createElement('h3');
  h3.textContent = title;
  div.append(h3, content);
  return div;
}

export function initXref(): void {
  const xref = window.__xref;
  if (!xref) return;
  const { imports, importedBy, symbols, self: selfHtml = '' } = xref;
  if (!imports?.length && !importedBy?.length && !symbols?.length) return;

  const panel = document.createElement('div');
  panel.id = 'xref-panel';
  const h2 = document.createElement('h2');
  h2.textContent = 'Module cross-references';
  panel.appendChild(h2);

  if (imports?.length)    panel.appendChild(section('Imports', linkList(imports, selfHtml)));
  if (importedBy?.length) panel.appendChild(section('Imported by', linkList(importedBy, selfHtml)));
  if (symbols?.length) {
    const sym = document.createElement('div');
    sym.className = 'xref-symbols';
    sym.textContent = symbols.join('  \u00b7  ');
    panel.appendChild(section('Public symbols', sym));
  }

  document.getElementById('content')?.appendChild(panel);
}
