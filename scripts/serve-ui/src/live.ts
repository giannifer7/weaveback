export function initLiveReload(): void {
  if (window.location.protocol === 'file:') return;

  let seen: string | null = null;
  let inFlight = false;

  async function poll() {
    if (inFlight) return;
    inFlight = true;
    try {
      const resp = await fetch('/__version', { cache: 'no-store' });
      if (!resp.ok) return;
      const current = (await resp.text()).trim();
      if (seen === null) {
        seen = current;
        return;
      }
      if (current !== seen) {
        location.reload();
      }
    } catch {
      // Ignore transient polling failures.
    } finally {
      inFlight = false;
    }
  }

  void poll();
  window.setInterval(() => { void poll(); }, 2000);
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
