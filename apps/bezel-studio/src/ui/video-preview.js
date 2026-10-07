import { el } from './dom.js';
import { errorText } from '../messages.js';

/** Local preview only: opening/closing this player never changes the theme or screen. */
export function openVideoPreview({ t, name, load, associate = null, gif = false }) {
  const opener = document.activeElement;
  const status = el('p', { class: 'hint', role: 'status', text: t('videoPreview.loading') });
  const player = el(gif ? 'img' : 'video', { class: 'video-preview-player', controls: !gif, loop: !gif, playsinline: !gif, alt: gif ? name : null, hidden: true });
  const close = el('button', { type: 'button', class: 'text-button', text: t('dialog.close'), onclick: () => dialog.close() });
  const link = associate && el('button', { type: 'button', class: 'text-button', text: t('videoPreview.associate'), hidden: true, onclick: () => { dialog.close(); associate(); } });
  const dialog = el('dialog', { class: 'confirm-dialog video-preview-dialog', 'aria-label': t('videoPreview.title', { name }) }, [
    el('div', { class: 'dialog-head' }, [el('h2', { text: t('videoPreview.title', { name }) })]),
    player, status,
    el('div', { class: 'dialog-actions' }, [link, close]),
  ]);
  player.addEventListener('error', () => {
    player.hidden = true;
    status.textContent = t('videoPreview.unsupported');
  });
  dialog.addEventListener('close', () => {
    if (!gif) player.pause();
    player.removeAttribute('src');
    if (!gif) player.load();
    dialog.remove();
    if (opener?.isConnected) opener.focus();
  }, { once: true });
  document.body.append(dialog);
  dialog.showModal();
  close.focus();
  Promise.resolve().then(load).then((url) => {
    if (!dialog.isConnected || !dialog.open) return;
    if (!url) {
      status.textContent = t('videoPreview.unavailable');
      if (link) link.hidden = false;
      return;
    }
    player.src = url;
    player.hidden = false;
    status.textContent = t('videoPreview.local');
  }).catch((error) => { if (dialog.isConnected) status.textContent = errorText(t, error); });
  return dialog;
}
