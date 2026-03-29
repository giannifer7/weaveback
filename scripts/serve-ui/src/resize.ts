export interface ResizeOpts {
  el: HTMLElement;
  storageKey: string;
  anchor: 'bottom-right' | 'bottom-left';
  minW?: number;
  minH?: number;
  defaultW: number;
  defaultH: number;
}

function clamp(v: number, lo: number, hi: number) {
  return Math.max(lo, Math.min(hi, v));
}

export function applyStoredSize(el: HTMLElement, storageKey: string, defaultW: number, defaultH: number): void {
  const w = parseInt(localStorage.getItem(`${storageKey}-w`) ?? '', 10);
  const h = parseInt(localStorage.getItem(`${storageKey}-h`) ?? '', 10);
  el.style.width  = `${w > 0 ? w : defaultW}px`;
  el.style.height = `${h > 0 ? h : defaultH}px`;
}

function makeDrag(
  handle: HTMLElement,
  el: HTMLElement,
  storageKey: string,
  axis: 'v' | 'h',
  invertH: boolean,
  minW: number,
  minH: number,
): void {
  handle.addEventListener('pointerdown', startEv => {
    startEv.preventDefault();
    handle.setPointerCapture(startEv.pointerId);
    handle.classList.add('dragging');

    const startX = startEv.clientX, startY = startEv.clientY;
    const startW = el.offsetWidth,  startH = el.offsetHeight;

    const onMove = (ev: PointerEvent) => {
      if (axis === 'v') {
        const h = clamp(startH + (startY - ev.clientY), minH, window.innerHeight - 40);
        el.style.height = `${h}px`;
        localStorage.setItem(`${storageKey}-h`, String(h));
      } else {
        const dx = invertH ? startX - ev.clientX : ev.clientX - startX;
        const w  = clamp(startW + dx, minW, window.innerWidth - 20);
        el.style.width = `${w}px`;
        localStorage.setItem(`${storageKey}-w`, String(w));
      }
    };

    const onUp = () => {
      handle.classList.remove('dragging');
      handle.releasePointerCapture(startEv.pointerId);
      handle.removeEventListener('pointermove', onMove);
      handle.removeEventListener('pointerup', onUp);
    };

    handle.addEventListener('pointermove', onMove);
    handle.addEventListener('pointerup', onUp);
  });
}

export function attachResizeHandles(opts: ResizeOpts): void {
  const { el, storageKey, anchor, minW = 280, minH = 120, defaultW, defaultH: _defaultH } = opts;

  // Top handle — vertical resize
  const top = document.createElement('div');
  top.className = 'wb-resize-n';
  el.prepend(top);
  makeDrag(top, el, storageKey, 'v', false, minW, minH);

  // Side handle — horizontal resize
  const side = document.createElement('div');
  side.className = anchor === 'bottom-right' ? 'wb-resize-w' : 'wb-resize-e';
  el.appendChild(side);
  makeDrag(side, el, storageKey, 'h', anchor === 'bottom-right', defaultW, minH);
}

export function addMaximizeToggle(
  btn: HTMLButtonElement,
  el: HTMLElement,
  storageKey: string,
  defaultW: number,
  defaultH: number,
): void {
  btn.textContent = '\u229e'; // ⊞
  btn.addEventListener('click', () => {
    const maxed = el.classList.toggle('maximized');
    btn.title     = maxed ? 'Restore panel' : 'Maximize panel';
    btn.textContent = maxed ? '\u229f' : '\u229e'; // ⊟ / ⊞
    if (!maxed) applyStoredSize(el, storageKey, defaultW, defaultH);
  });
}
