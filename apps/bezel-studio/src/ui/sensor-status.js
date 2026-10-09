// The LibreHardwareMonitor dot in the statusbar (artboard SensorStatus): a
// button whose label says how the reader is doing; it opens a small dialog
// with each hardware group's state and "Restart", which restarts only the
// Libre reader (D-2026-10-09-studio-redesign-10). Escape or a click outside
// closes it; Escape gives the focus back to the dot.
import { el } from './dom.js';

/**
 * @param {{
 *   t: (k: string, p?: object) => string,
 *   onOpenSensors: () => void,
 * }} deps
 * @returns {{ update: (health: {state: string, failed: string[], hardware?: {name: string, ok: boolean}[]} | null, restarting: boolean) => void, close: () => void }}
 */
export function createSensorStatus({ t, onOpenSensors }) {
  const wrap = document.getElementById('status-libre');
  const toggle = document.getElementById('status-libre-toggle');
  const popover = document.getElementById('libre-popover');
  const list = document.getElementById('libre-hardware');
  let last = { health: null, restarting: false };

  const isOpen = () => !popover.hidden;

  function place() {
    const box = toggle.getBoundingClientRect();
    const right = Math.max(8, window.innerWidth - box.right);
    popover.style.right = `${right}px`;
    popover.style.bottom = `${window.innerHeight - box.top + 8}px`;
  }

  function open() {
    render();
    popover.hidden = false;
    toggle.setAttribute('aria-expanded', 'true');
    place();
    const restart = document.getElementById('restart-libre');
    (restart.disabled ? popover.querySelector('.libre-popover-close') : restart).focus();
  }

  function close({ refocus = false } = {}) {
    if (!isOpen()) return;
    popover.hidden = true;
    toggle.setAttribute('aria-expanded', 'false');
    if (refocus) toggle.focus();
  }

  function row({ name, ok }) {
    return el('li', { class: 'libre-row', dataset: { state: ok ? 'ok' : 'missing' } }, [
      el('span', { class: 'status-dot', dataset: { state: ok ? 'ok' : 'partial' }, 'aria-hidden': 'true' }),
      el('span', { class: 'libre-row-name' }, [
        el('span', { text: name }),
        el('span', { class: 'libre-row-via', text: t('sensorStatus.via') }),
      ]),
      el('span', { class: 'libre-row-value', text: t(ok ? 'sensorStatus.reading' : 'sensorStatus.missing') }),
    ]);
  }

  function render() {
    const { health, restarting } = last;
    if (!health) return;
    const status = restarting ? 'restarting' : health.state;
    document.getElementById('libre-popover-dot').dataset.state = status;
    document.getElementById('libre-popover-title').textContent = t(`status.libre.${status}`);
    list.replaceChildren(...(health.hardware ?? []).map(row));
    const note = document.getElementById('libre-popover-note');
    note.hidden = !health.failed.length;
    note.textContent = health.failed.length ? t('sensorStatus.hint', { hardware: health.failed.join(', ') }) : '';
  }

  toggle.addEventListener('click', () => (isOpen() ? close() : open()));
  popover.querySelector('.libre-popover-close').addEventListener('click', () => close({ refocus: true }));
  document.getElementById('libre-open-sensors').addEventListener('click', () => {
    close();
    onOpenSensors();
  });
  // Escape closes it from inside, from the dot, or when the focus was lost
  // (the restart button turns disabled while the reader restarts).
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape' || !isOpen()) return;
    const at = document.activeElement;
    if (at && at !== document.body && !popover.contains(at) && at !== toggle) return;
    e.preventDefault();
    e.stopPropagation();
    close({ refocus: true });
  }, true);
  document.addEventListener('pointerdown', (e) => {
    if (isOpen() && !popover.contains(e.target) && !toggle.contains(e.target)) close();
  });
  window.addEventListener('resize', () => isOpen() && place());

  return {
    update(health, restarting) {
      // While the reader restarts there are no readings: the list keeps the last one.
      last = { health: restarting && health && last.health ? last.health : health, restarting };
      wrap.hidden = !health;
      if (!health) {
        close();
        return;
      }
      if (isOpen()) render();
    },
    close,
  };
}
