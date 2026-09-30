// What fixes a port the system denied (D-2026-09-30-release-polish-3): on
// Linux the backend names the one-line command that installs Bezel's udev
// rule. The UI shows it, with a Copy button; Bezel never runs it.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { errorText } from '../messages.js';

/**
 * The command in a code block with a Copy button. When the clipboard is
 * refused, the command is selected for Ctrl+C.
 * @param {(k: string, p?: object) => string} t
 * @param {string} command
 * @param {(message: string) => void} notify
 */
export function udevCommand(t, command, notify) {
  const code = el('code', { class: 'udev-command-text', text: command });
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(command);
      notify(t('udev.copied'));
    } catch {
      const range = document.createRange();
      range.selectNodeContents(code);
      window.getSelection()?.removeAllRanges();
      window.getSelection()?.addRange(range);
      notify(t('udev.copyFailed'));
    }
  };
  return el('div', { class: 'udev-command', role: 'group', 'aria-label': t('udev.command') }, [
    code,
    el('button', { type: 'button', class: 'text-button', onclick: copy }, [icon(ICONS.copy, 16), t('udev.copy')]),
  ]);
}

/**
 * Explains a denied port in a modal dialog, with the command that fixes it.
 * @param {(k: string, p?: object) => string} t
 * @param {{udevCommand: string}} error an `accessDenied` error
 * @param {(message: string) => void} notify
 * @returns {Promise<void>} once the dialog is closed
 */
export function showAccessHelp(t, error, notify) {
  return new Promise((resolve) => {
    const opener = document.activeElement;
    const done = el('button', { type: 'button', class: 'primary-button', text: t('dialog.close') });
    const close = el('button', { type: 'button', class: 'icon-button dialog-close', title: t('dialog.close'), 'aria-label': t('dialog.close') }, [icon(ICONS.close, 16)]);
    const dialog = el('dialog', { class: 'confirm-dialog udev-dialog', 'aria-labelledby': 'udev-title', 'aria-describedby': 'udev-why' }, [
      el('div', { class: 'dialog-head' }, [el('h2', { id: 'udev-title', text: t('udev.title') }), close]),
      el('div', { class: 'dialog-body' }, [
        el('p', { id: 'udev-why', text: errorText(t, error) }),
        el('p', { text: t('udev.explain') }),
        udevCommand(t, error.udevCommand, notify),
        el('p', { class: 'hint', text: t('udev.never') }),
      ]),
      el('div', { class: 'dialog-actions' }, [done]),
    ]);
    done.addEventListener('click', () => dialog.close());
    close.addEventListener('click', () => dialog.close());
    dialog.addEventListener('close', () => {
      dialog.remove();
      if (opener?.isConnected) opener.focus();
      resolve();
    }, { once: true });
    document.body.append(dialog);
    dialog.showModal();
    dialog.querySelector('.udev-command button').focus();
  });
}
