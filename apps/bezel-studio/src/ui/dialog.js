// A modal choice dialog: focus stays inside, Esc and the close button
// cancel, and focus returns to the element that opened it.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';

const BUTTON_CLASS = { primary: 'primary-button', danger: 'danger-button', text: 'text-button' };

/**
 * Asks a question; resolves with the `id` of the chosen action, or
 * `'cancel'`.
 * @param {(k: string, p?: object) => string} t
 * @param {{title: string, body: string, actions: Array<{id: string, label: string, kind?: 'primary'|'danger'|'text'}>, initial: string}} opts
 *   `actions` in reading order after Cancel; `initial` gets the focus.
 * @returns {Promise<string>}
 */
export function askChoice(t, { title, body, actions, initial }) {
  return new Promise((resolve) => {
    const opener = document.activeElement;
    const cancel = el('button', { type: 'button', class: 'text-button', text: t('dialog.cancel') });
    const buttons = actions.map((a) => el('button', { type: 'button', class: BUTTON_CLASS[a.kind ?? 'text'], dataset: { choice: a.id }, text: a.label }));
    const close = el('button', { type: 'button', class: 'icon-button dialog-close', title: t('dialog.close'), 'aria-label': t('dialog.close') }, [icon(ICONS.close, 16)]);
    const dialog = el('dialog', { class: 'confirm-dialog', 'aria-labelledby': 'choice-title', 'aria-describedby': 'choice-body' }, [
      el('div', { class: 'dialog-head' }, [el('h2', { id: 'choice-title', text: title }), close]),
      el('div', { id: 'choice-body', class: 'dialog-body' }, [el('p', { text: body })]),
      el('div', { class: 'dialog-actions' }, [cancel, ...buttons]),
    ]);
    cancel.addEventListener('click', () => dialog.close('cancel'));
    close.addEventListener('click', () => dialog.close('cancel'));
    for (const b of buttons) b.addEventListener('click', () => dialog.close(b.dataset.choice));
    dialog.addEventListener('close', () => {
      const answer = dialog.returnValue || 'cancel';
      dialog.remove();
      if (opener?.isConnected) opener.focus();
      resolve(answer);
    }, { once: true });
    document.body.append(dialog);
    dialog.showModal();
    (buttons.find((b) => b.dataset.choice === initial) ?? cancel).focus();
  });
}
