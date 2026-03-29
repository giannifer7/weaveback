export function initToc(): void {
  const toc = document.querySelector<HTMLElement>('#toc.toc2');
  if (!toc) return;

  const btn = document.createElement('button');
  btn.id = 'toc-toggle';
  btn.setAttribute('aria-label', 'Toggle table of contents');
  toc.appendChild(btn);

  let collapsed = localStorage.getItem('weaveback-toc-collapsed') === '1';

  function apply() {
    document.body.classList.toggle('toc-collapsed', collapsed);
    btn.textContent = collapsed ? '\u25b6' : '\u25c0';
  }

  apply();
  btn.addEventListener('click', () => {
    collapsed = !collapsed;
    localStorage.setItem('weaveback-toc-collapsed', collapsed ? '1' : '0');
    apply();
  });
}
