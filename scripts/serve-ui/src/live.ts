export function initLiveReload(): void {
  if (typeof EventSource === 'undefined' || window.location.protocol === 'file:') return;

  function connect() {
    const es = new EventSource('/__events');
    es.addEventListener('reload', () => location.reload());
    es.onerror = () => { es.close(); setTimeout(connect, 2000); };
  }
  connect();
}

export function initEditButton(): void {
  if (window.location.protocol === 'file:') return;
  const path = window.location.pathname.replace(/\.html$/, '.adoc').replace(/^\//, '');
  if (!path) return;

  const btn = document.createElement('a');
  btn.id = 'wb-edit-btn';
  btn.href = '#';
  btn.textContent = '\u270f Edit source';
  btn.title = path;
  btn.addEventListener('click', e => {
    e.preventDefault();
    void fetch(`/__open?file=${encodeURIComponent(path)}&line=1`);
  });
  document.body.appendChild(btn);
}
