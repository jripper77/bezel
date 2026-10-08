// "When the computer shuts down" under Screen › Settings
// (D-2026-10-03-power-off-standby-6): the four choices of the screen chosen
// at the top as a radio group, each explained by its "?" in a popover;
// an option the screen does not offer is disabled and says why. Choosing
// opens an in-app dialog that says what happens at shutdown and exactly what
// is written to the screen now (its plan B): `off` asks the minutes, `video`
// lists the screen's videos, `album` opens the card album's manager, where
// photos are added with a preview in the shape the screen stands in, and
// removed after a confirmation that names them (D-2026-10-03-power-off-
// standby-4). Nothing is written without that confirmation; `keep` while it
// is the choice sends nothing.
//
// The arrow keys move the focus along the group without choosing: choosing
// opens a dialog, so it takes Space or Enter (or a click), not an arrow.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { createConfirm } from './storage.js';
import { errorText } from '../messages.js';
import { formatBytes, planRefusalText } from '../storage-manager.js';
import {
  CHOICES, PHOTO_FITS, activation, albumClash, albumName, albumPath, albumPhotos, complete, confirmationOf, createAnswers, currentDetail,
  dialogDefaults, inTurn, minutesText, offered, optionOf, requestOf, screenShape, sleepChoices, suggestPhotoName, translate, videoGroups,
} from '../standby.js';

/** The icon of each choice. */
const CHOICE_ICONS = Object.freeze({
  keep: ['M4 5h16v11H4z', 'M9 20h6', 'M12 16v4'],
  off: ICONS.power,
  video: ICONS.film,
  album: ICONS.image,
});

/** The keys that move the focus along the radio group, as a step (or an end). */
const MOVES = Object.freeze({ ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1, Home: 'first', End: 'last' });

let dialogs = 0;

/**
 * The "?" buttons and their popovers: one open at a time; Esc closes it (not
 * the dialog it is in) and gives the focus back to its "?", and so does a
 * click outside that leaves the focus nowhere.
 */
function createPopovers() {
  let open = null;
  let swallowCancel = false;

  function close({ refocus = false } = {}) {
    if (!open) return;
    const { button, panel } = open;
    open = null;
    panel.hidden = true;
    button.setAttribute('aria-expanded', 'false');
    if (refocus && button.isConnected) button.focus();
  }

  function toggle(button, panel) {
    const same = open?.panel === panel;
    close();
    if (same) return;
    open = { button, panel };
    panel.hidden = false;
    button.setAttribute('aria-expanded', 'true');
    panel.focus();
  }

  function escape(evt) {
    if (evt.key !== 'Escape' || !open) return;
    evt.preventDefault();
    evt.stopPropagation();
    swallowCancel = true;
    close({ refocus: true });
  }

  document.addEventListener('click', (evt) => {
    if (!open || open.panel.contains(evt.target) || open.button.contains(evt.target)) return;
    const active = document.activeElement;
    const lost = !active || active === document.body || open.panel.contains(active);
    close({ refocus: lost });
  });

  return {
    /** A "?" that opens `content` in a popover named `label`: `[button, panel]`. */
    help(id, label, content) {
      const panel = el('div', { id, class: 'popover', role: 'group', 'aria-label': label, tabindex: '-1', hidden: true, onkeydown: escape }, content);
      const button = el('button', {
        type: 'button', class: 'icon-button help-button', text: '?', title: label, 'aria-label': label,
        'aria-expanded': 'false', 'aria-controls': id, dataset: { focus: `help-${id}` }, onkeydown: escape,
      });
      button.addEventListener('click', () => toggle(button, panel));
      return [button, panel];
    },
    close,
    /** Esc that closed a popover inside `dialog` does not close the dialog too. */
    guard(dialog) {
      dialog.addEventListener('cancel', (evt) => {
        if (!swallowCancel) return;
        swallowCancel = false;
        evt.preventDefault();
      });
      dialog.addEventListener('keydown', () => { swallowCancel = false; }, true);
    },
  };
}

/**
 * @param {object} deps
 * @param {HTMLElement} deps.root the section it draws
 * @param {(k: string, p?: object) => string} deps.t
 * @param {() => string} deps.locale the UI's language now
 * @param {object} deps.bridge
 * @param {(message: string) => void} deps.notify short confirmation (toast)
 * @param {() => {screen: object|null}} deps.context the screen chosen at the top
 * @param {() => void} [deps.storageChanged] the album changed what the screen stores
 */
export function createStandbyPanel({ root, t, locale, bridge, notify, context, storageChanged = () => {} }) {
  const view = { key: null, signature: '', shown: false, status: 'idle', data: null, error: null, busy: false, problem: null };
  // Which answer about the screen shown is the newest: a reading or a write.
  const answers = createAnswers();
  const popovers = createPopovers();
  const confirm = createConfirm(t, { refocus: () => focusRadio(view.data?.choice ?? 'keep') });
  const screen = () => context().screen;
  const say = (message) => translate(t, message);
  const bytes = (n) => formatBytes(n, locale());

  // ------------------------------------------------------------ loading --
  async function load() {
    const current = screen();
    if (!current) return render();
    const key = current.key;
    const ticket = answers.reading();
    if (view.key !== key || !view.data) view.status = 'loading';
    render();
    try {
      const data = await bridge.standbyOverview(key);
      if (!answers.newest(ticket)) return;
      Object.assign(view, { data, status: 'ready', error: null });
    } catch (e) {
      if (!answers.newest(ticket)) return;
      Object.assign(view, { status: 'error', error: e });
    }
    render();
  }

  // ---------------------------------------------------------- drawing --
  /** The control with the focus, to give it back after a redraw. */
  function focusedKey() {
    const active = document.activeElement;
    return root.contains(active) ? active.closest('[data-focus]')?.dataset.focus ?? null : null;
  }

  function radios() {
    return [...root.querySelectorAll('[role="radio"]')];
  }

  /** Moves the focus (and the group's tab stop) to the radio of `choice`. */
  function focusRadio(choice) {
    const target = radios().find((r) => r.dataset.choice === choice) ?? radios()[0];
    if (!target) return;
    for (const r of radios()) r.tabIndex = r === target ? 0 : -1;
    target.focus();
  }

  function moveFocus(evt, choice) {
    const move = MOVES[evt.key];
    if (move === undefined) return;
    evt.preventDefault();
    const i = CHOICES.indexOf(choice);
    let next = move;
    if (move === 'first') next = -i;
    if (move === 'last') next = CHOICES.length - 1 - i;
    focusRadio(CHOICES[(i + next + CHOICES.length) % CHOICES.length]);
  }

  function optionRow(choice) {
    const option = optionOf(view.data, choice);
    const checked = view.data.choice === choice;
    const label = t(`standby.choice.${choice}`);
    const detail = option.enabled ? currentDetail(view.data, choice) : { key: `standby.reason.${option.reason}`, params: {} };
    const nameId = `standby-name-${choice}`;
    const detailId = `standby-detail-${choice}`;
    const radio = el('button', {
      type: 'button', role: 'radio', class: 'standby-radio', 'aria-checked': String(checked), 'aria-disabled': option.enabled ? null : 'true',
      'aria-labelledby': nameId, 'aria-describedby': detail ? detailId : null, tabindex: checked ? '0' : '-1',
      title: option.enabled ? null : say(detail), dataset: { choice, focus: `radio-${choice}` },
      onclick: () => void choose(choice), onkeydown: (evt) => moveFocus(evt, choice),
    }, [
      el('span', { class: 'standby-mark', 'aria-hidden': 'true' }),
      icon(CHOICE_ICONS[choice], 16),
      el('span', { id: nameId, class: 'standby-label', text: label }),
      detail && el('span', { id: detailId, class: `standby-detail${option.enabled ? '' : ' reason'}`, text: say(detail) }),
    ]);
    const [help, panel] = popovers.help(`standby-help-${choice}`, t('standby.helpFor', { choice: label }), [
      el('strong', { text: label }),
      el('p', { text: t(`standby.explain.${choice}`) }),
    ]);
    return el('div', { class: `standby-option${checked ? ' checked' : ''}` }, [radio, help, panel]);
  }

  function head(current) {
    const [help, panel] = popovers.help('standby-help', t('standby.help'), [el('p', { text: t('standby.helpText') })]);
    const model = current.models.length === 1 ? current.models[0].name : current.models.map((m) => m.name).join(' / ');
    return [
      el('div', { class: 'standby-head' }, [icon(ICONS.power, 16), el('h3', { id: 'standby-title', text: t('standby.title') }), help, panel]),
      el('p', { class: 'standby-screen', text: model }),
    ];
  }

  function bodyParts() {
    if (view.status === 'loading' || (view.status === 'idle' && !view.data)) return [el('p', { class: 'hint', role: 'status', text: t('standby.loading') })];
    if (view.status === 'error') {
      return [
        el('p', { class: 'field-error', role: 'status', text: t('standby.loadError', { message: errorText(t, view.error) }) }),
        el('div', { class: 'button-row' }, [el('button', { type: 'button', class: 'text-button', text: t('standby.retry'), dataset: { focus: 'retry' }, onclick: () => void load() })]),
      ];
    }
    if (!offered(view.data)) return [el('p', { class: 'hint', text: t('standby.unsupported') })];
    const parts = [el('div', { class: 'standby-options', role: 'radiogroup', 'aria-labelledby': 'standby-title', 'aria-busy': String(view.busy) }, CHOICES.map(optionRow))];
    const off=optionOf(view.data,'off');
    parts.push(el('section',{class:'sleep-settings'},[
      el('h3',{text:t('sleep.title')}),
      el('p',{class:'hint',text:t('sleep.help')}),
      el('button',{type:'button',class:'text-button',id:'configure-sleep-timer',text:t('sleep.configure'),disabled:view.busy || !off?.enabled,onclick:async()=>{
        const key=view.key;const request=await askChoice('off');if(request)await write(request,key);
      }}),
    ]));
    if (view.data.choice === 'album' && optionOf(view.data, 'album').enabled) {
      parts.push(el('div', { class: 'button-row' }, [el('button', {
        type: 'button', class: 'text-button', dataset: { focus: 'manage' }, disabled: view.busy, onclick: () => void manageAlbum({ choosing: false }),
      }, [icon(ICONS.image, 16), el('span', { text: t('standby.manageAlbum') })])]));
    }
    if (view.problem) parts.push(el('p', { class: 'field-error', role: 'alert', text: view.problem }));
    return parts;
  }

  function render() {
    const current = screen();
    root.hidden = !current;
    if (!current) {
      root.replaceChildren();
      return;
    }
    const focus = focusedKey();
    popovers.close();
    root.setAttribute('aria-busy', String(view.status === 'loading' || view.busy));
    root.replaceChildren(...head(current), ...bodyParts());
    if (focus) root.querySelector(`[data-focus="${focus}"]`)?.focus();
  }

  // ---------------------------------------------------------- dialogs --
  /**
   * A modal dialog; its footer's buttons close it (an `act` that answers false
   * keeps it open). `hold(true)` keeps it open while something it started runs:
   * every button disabled, Esc ignored.
   */
  function modal(title, { wide = false } = {}) {
    const opener = document.activeElement;
    dialogs += 1;
    const id = `standby-dialog-${dialogs}`;
    const body = el('div', { id: `${id}-body`, class: 'dialog-body' });
    const actions = el('div', { class: 'dialog-actions' });
    const close = el('button', { type: 'button', class: 'icon-button dialog-close', title: t('dialog.close'), 'aria-label': t('dialog.close') }, [icon(ICONS.close, 16)]);
    const dialog = el('dialog', { class: `confirm-dialog standby-dialog${wide ? ' wide-dialog' : ''}`, 'aria-labelledby': `${id}-title` }, [
      el('div', { class: 'dialog-head' }, [el('h2', { id: `${id}-title`, text: title }), close]),
      body,
      actions,
    ]);
    popovers.guard(dialog);
    let held = false;
    dialog.addEventListener('cancel', (evt) => {
      if (held) evt.preventDefault();
    });
    dialog.addEventListener('keydown', (evt) => {
      if (held && evt.key === 'Escape') evt.preventDefault();
    }, true);
    const hold = (on) => {
      held = on;
      for (const b of [close, ...actions.querySelectorAll('button')]) b.disabled = on;
    };
    let resolve;
    const result = new Promise((r) => { resolve = r; });
    close.addEventListener('click', () => dialog.close('cancel'));
    dialog.addEventListener('close', () => {
      popovers.close();
      dialog.remove();
      if (opener?.isConnected) opener.focus();
      else focusRadio(view.data?.choice ?? 'keep');
      resolve(dialog.returnValue || 'cancel');
    }, { once: true });
    const button = (choice, label, kind = 'text', act = null) => {
      const b = el('button', { type: 'button', class: { primary: 'primary-button', danger: 'danger-button' }[kind] ?? 'text-button', text: label });
      b.addEventListener('click', async () => {
        if (act && !(await act())) return;
        dialog.close(choice);
      });
      actions.append(b);
      return b;
    };
    document.body.append(dialog);
    dialog.showModal();
    return { dialog, body, result, button, hold, id };
  }

  /** What confirming `request` does: at shutdown, and written to the screen now. */
  function summary(request) {
    const { atShutdown, written } = confirmationOf(request);
    return [
      el('dt', { text: t('standby.summary.atShutdown') }),
      el('dd', { text: say(atShutdown) }),
      el('dt', { text: t('standby.summary.written') }),
      el('dd', {}, [el('ul', { class: 'standby-written' }, written.map((line) => el('li', { text: say(line) })))]),
    ];
  }

  function minutesField(m, values, refresh) {
    const select = el('select', { id: `${m.id}-minutes` }, sleepChoices().map((n) => el('option', { value: String(n), text: say(minutesText(n)), selected: n === values.sleepMinutes })));
    select.addEventListener('change', () => {
      values.sleepMinutes = Number(select.value);
      refresh();
    });
    const label = t('standby.off.minutes');
    const [help, panel] = popovers.help(`${m.id}-minutes-help`, t('standby.off.minutesHelp'), [el('p', { text: t('standby.off.minutesExplain') })]);
    return el('div', { class: 'field standby-field' }, [
      el('div', { class: 'standby-field-head' }, [el('label', { for: select.id, text: label }), help, panel]),
      select,
    ]);
  }

  function videoField(m, values, refresh) {
    const groups = videoGroups(view.data.videos).map(({ medium, files }) => el('div', { class: 'video-group', role: 'group', 'aria-label': t(`storage.medium.${medium}`) }, [
      el('p', { class: 'video-medium', 'aria-hidden': 'true', text: t(`storage.medium.${medium}`) }),
      ...files.map((v) => {
        const input = el('input', { type: 'radio', name: `${m.id}-video`, value: v.path, checked: v.path === values.file });
        input.addEventListener('change', () => {
          values.file = v.path;
          refresh();
        });
        return el('label', { class: 'check video-choice' }, [input, el('span', { text: v.name }), el('small', { text: bytes(v.size) })]);
      }),
    ]));
    return el('fieldset', { class: 'check-list standby-videos' }, [el('legend', { text: t('standby.video.list') }), ...groups]);
  }

  /** Asks the confirmation of `keep`, `off` or `video`: the request, or `null`. */
  async function askChoice(choice) {
    const m = modal(t(`standby.${choice}.title`));
    const values = dialogDefaults(view.data, choice);
    const facts = el('dl', { class: 'summary standby-summary' });
    const refresh = () => facts.replaceChildren(...summary(requestOf(choice, values)));
    if (choice === 'off') m.body.append(minutesField(m, values, refresh));
    if (choice === 'video') m.body.append(videoField(m, values, refresh));
    m.body.append(facts);
    refresh();
    m.button('cancel', t('dialog.cancel'));
    const ok = m.button('ok', t('standby.write'), 'primary');
    (m.body.querySelector('select, input:checked') ?? ok).focus();
    if ((await m.result) !== 'ok') return null;
    const request = requestOf(choice, values);
    return complete(request) ? request : null;
  }

  /**
   * Writes `request` to the screen `key` (the one shown when it was asked).
   * Its answer is drawn only while that screen is still the one shown, and
   * only if no reading of it started since; if one did (the screen was
   * chosen again at the top while the write ran), it may have read the
   * choice from before the write, so the screen is read again
   * (`answers.written`). Written for a screen no longer shown, it leaves the
   * reading of the shown one alone.
   */
  async function write(request, key = view.key) {
    const ticket = answers.writing(key, view.key);
    view.busy = true;
    if (view.key === key) view.problem = null;
    render();
    let data = null;
    let problem = null;
    try {
      data = await bridge.setStandby(key, request, true);
      notify(t('standby.saved', { choice: t(`standby.choice.${request.choice}`) }));
    } catch (e) {
      problem = errorText(t, e);
    }
    view.busy = false;
    const same = view.key === key;
    const next = answers.written(ticket, key, view.key);
    if (next === 'draw' && data) Object.assign(view, { data, status: 'ready', error: null });
    if (same) view.problem = problem;
    render();
    if (next === 'read' && view.shown) void load();
    if (same) focusRadio(view.data?.choice ?? request.choice);
  }

  async function choose(choice) {
    if (view.busy || !view.data) return;
    view.problem = null;
    const act = activation(view.data, choice);
    if (act === 'refused' || act === 'nothing') return;
    if (choice === 'album') {
      await manageAlbum({ choosing: act === 'ask' });
      return;
    }
    // The screen the confirmation is about, whatever is shown once it is answered.
    const { key } = view;
    const request = await askChoice(choice);
    if (request) await write(request, key);
  }

  // ------------------------------------------------------------ album --
  /**
   * The card album's manager: its photos, Add and Remove; when choosing the
   * album, also what is written and "Use the album".
   */
  async function manageAlbum({ choosing }) {
    // The screen as it stood when the manager opened: its key, panel and orientation.
    const standing = { key: view.key, model: screen().models[0], orientation: view.data.orientation };
    const { key } = standing;
    const m = modal(t('standby.album.title'), { wide: true });
    let photos = [];
    const status = el('p', { class: 'hint', role: 'status' });
    const list = el('ul', { class: 'album-photos', 'aria-labelledby': `${m.id}-photos` });
    const add = el('button', { type: 'button', class: 'text-button', dataset: { focus: 'album-add' } }, [icon(ICONS.upload, 16), el('span', { text: t('standby.album.add') })]);
    const [help, panel] = popovers.help(`${m.id}-help`, t('standby.album.help'), [el('p', { text: t('standby.album.explain') })]);
    m.body.append(
      el('div', { class: 'standby-field-head' }, [el('h3', { id: `${m.id}-photos`, class: 'dialog-subtitle', text: t('standby.album.photos') }), help, panel]),
      status,
      list,
      el('div', { class: 'button-row' }, [add]),
    );
    let use = null;
    const need = el('p', { class: 'hint album-need', text: t('standby.album.needsPhoto') });
    if (choosing) m.body.append(el('dl', { class: 'summary standby-summary' }, summary(requestOf('album'))), need);
    m.button('cancel', choosing ? t('dialog.cancel') : t('dialog.close'));
    if (choosing) use = m.button('ok', t('standby.album.use'), 'primary');

    // One thumbnail at a time, on purpose, like the storage manager's queue:
    // the studio draws them behind one lock, so asking for every photo at
    // once would only park a blocking thread per photo on it.
    function thumbnails() {
      return inTurn(list.querySelectorAll('img[data-path]'), thumbnailOf);
    }

    async function thumbnailOf(img) {
      const url = await Promise.resolve(bridge.managerThumbnail(key, img.dataset.path)).catch(() => null);
      if (!url || !img.isConnected) return;
      img.src = url;
      img.closest('.album-thumb')?.classList.add('has-picture');
    }

    function photoItem(photo) {
      const remove = el('button', {
        type: 'button', class: 'icon-button', title: t('standby.album.remove', { name: photo.name }), 'aria-label': t('standby.album.remove', { name: photo.name }),
        dataset: { photo: photo.name }, onclick: () => void removePhoto(photo),
      }, [icon(ICONS.trash, 16)]);
      return el('li', { class: 'album-photo' }, [
        el('span', { class: 'album-thumb', 'aria-hidden': 'true' }, [el('img', { alt: '', dataset: { path: photo.path } }), icon(ICONS.image, 20)]),
        el('span', { class: 'album-info' }, [el('span', { class: 'album-name', text: photo.name }), el('small', { text: bytes(photo.size) })]),
        remove,
      ]);
    }

    function draw() {
      list.replaceChildren(...photos.map(photoItem));
      if (!photos.length) list.append(el('li', { class: 'empty-note', text: t('standby.album.empty') }));
      if (use) use.disabled = photos.length === 0;
      need.hidden = !choosing || photos.length > 0;
      void thumbnails();
    }

    // The listing being read: adding waits for it, as it says what a name would replace.
    let listing = Promise.resolve();

    /** Reads the album again; `note` (a failure to show) stays under it once read. */
    function reload(note = '') {
      listing = (async () => {
        status.textContent = t('standby.album.loading');
        try {
          photos = albumPhotos(await bridge.managerOverview(key));
          status.textContent = note;
        } catch (e) {
          status.textContent = [note, t('standby.album.loadError', { message: errorText(t, e) })].filter(Boolean).join(' ');
        }
        draw();
      })();
      return listing;
    }

    async function removePhoto(photo) {
      const at = photos.indexOf(photo);
      const ok = await confirm({
        title: t('standby.album.removeTitle', { name: photo.name }),
        body: [el('p', { text: t('storage.confirmDelete', { name: photo.name, place: t('storage.from.sd') }) })],
        action: t('standby.album.removeAction'),
        danger: true,
      });
      if (!ok) return;
      // A removal that failed says why, and that stays after the album is read again.
      let failed = '';
      try {
        await bridge.deleteStored(key, photo.path, true);
        notify(t('standby.album.removed', { name: photo.name }));
        storageChanged();
      } catch (e) {
        failed = errorText(t, e);
      }
      await reload(failed);
      const next = list.querySelectorAll('button[data-photo]')[Math.min(at, photos.length - 1)];
      (next ?? add).focus();
    }

    add.addEventListener('click', async () => {
      let source = null;
      try {
        source = await bridge.pickPhoto();
      } catch (e) {
        status.textContent = errorText(t, e);
      }
      if (!source) return;
      await listing;
      // What follows a photo sent is done once it is on the card, whatever became of its dialog.
      const added = (name) => {
        notify(t('standby.add.added', { name }));
        storageChanged();
        void reload();
      };
      await askAdd(standing, source, photos, { stale: () => void reload(), added });
    });

    await reload();
    (photos.length ? list.querySelector('button[data-photo]') : add).focus();
    if ((await m.result) === 'ok' && choosing && photos.length) await write(requestOf('album'), key);
  }

  /**
   * The preview of `source` framed for the screen of `target` (`{key, model,
   * orientation}`), its fit and name, and Send: `{name}` once sent, else `null`.
   * A name among `photos` is sent as a replacement only after its own
   * confirmation (the danger button); the app checks the card's listing
   * too, and a name it says is taken (`notConfirmed`) turns the dialog into
   * that confirmation, `stale` telling the album its list was not current.
   * While it sends, the dialog stays open (nothing closes it) and `added`
   * gets the name as soon as the photo is on the card.
   */
  async function askAdd({ key, model, orientation }, source, photos, { stale = () => {}, added = () => {} } = {}) {
    // The names the album has, as far as this dialog knows.
    const taken = [...photos];
    const file = source.split(/[/\\]/).pop();
    const m = modal(t('standby.add.title', { name: file }), { wide: true });
    const shape = screenShape(model, orientation);
    let fit = PHOTO_FITS[0];
    let shown = 0;
    const img = el('img', { alt: t('standby.add.previewAlt', { name: file }) });
    const frame = el('div', { class: 'album-frame', dataset: { axis: shape.axis, fit, state: 'loading' }, style: { aspectRatio: `${shape.width} / ${shape.height}` } }, [img]);
    const previewNote = el('p', { class: 'hint', role: 'status' });
    const fits = PHOTO_FITS.map((f) => el('button', {
      type: 'button', 'aria-pressed': String(f === fit), title: t(`standby.add.${f}Hint`), dataset: { fit: f }, text: t(`framing.${f}`),
      onclick: () => {
        fit = f;
        for (const b of fits) b.setAttribute('aria-pressed', String(b.dataset.fit === fit));
        void preview();
      },
    }));
    const input = el('input', { id: `${m.id}-name`, type: 'text', spellcheck: 'false', value: suggestPhotoName(source), 'aria-describedby': `${m.id}-target` });
    const target = el('p', { id: `${m.id}-target`, class: 'hint' });
    const clash = el('p', { class: 'dialog-warning', hidden: true }, [icon(ICONS.warning, 18), el('span')]);
    const error = el('p', { class: 'field-error', role: 'alert' });

    async function preview() {
      const asked = (shown += 1);
      frame.dataset.state = 'loading';
      frame.dataset.fit = fit;
      previewNote.textContent = t('standby.add.previewing');
      try {
        const url = await bridge.albumPreview(key, source, fit);
        if (asked !== shown) return;
        img.src = url;
        frame.dataset.state = 'ready';
        previewNote.textContent = t(shape.axis === 'horizontal' ? 'standby.add.onHorizontal' : 'standby.add.onVertical');
      } catch (e) {
        if (asked !== shown) return;
        frame.dataset.state = 'none';
        previewNote.textContent = t('standby.add.previewError', { message: errorText(t, e) });
      }
    }

    m.body.append(
      el('figure', { class: 'album-preview' }, [frame, el('figcaption', {}, [previewNote])]),
      el('div', { class: 'field' }, [el('span', { id: `${m.id}-fit`, text: t('framing.fit') }), el('div', { class: 'segmented album-fit', role: 'group', 'aria-labelledby': `${m.id}-fit` }, fits)]),
      el('div', { class: 'field' }, [el('label', { for: input.id, text: t('standby.add.name') }), input, target]),
      clash,
      error,
    );
    m.button('cancel', t('dialog.cancel'));
    let sent = null;
    const send = m.button('ok', t('standby.add.send'), 'primary', async () => {
      const { name, problem } = albumName(input.value);
      if (problem) return false;
      const replace = Boolean(albumClash(taken, name));
      m.hold(true);
      error.textContent = '';
      previewNote.textContent = t('standby.add.sending');
      try {
        await bridge.albumAdd(key, source, fit, name, true, replace);
        sent = { name };
        added(name);
        return true;
      } catch (e) {
        m.hold(false);
        previewNote.textContent = '';
        if (e?.code === 'notConfirmed' && !replace) {
          // The card has that name: replacing it is asked first.
          taken.push({ name });
          stale();
        } else {
          error.textContent = errorText(t, e);
        }
        check();
        return false;
      }
    });

    // Whether Send is the danger button now: replacing is never what Enter does.
    let replacing = false;

    function check() {
      const { name, problem } = albumName(input.value);
      const replaces = problem ? null : albumClash(taken, name);
      replacing = Boolean(replaces);
      target.textContent = problem ? planRefusalText(t, locale(), problem) : t('standby.add.sendsTo', { path: albumPath(name) });
      target.classList.toggle('field-error', Boolean(problem));
      input.setAttribute('aria-invalid', String(Boolean(problem)));
      clash.hidden = !replaces;
      clash.querySelector('span').textContent = replaces ? t('standby.add.replaces', { name: replaces.name }) : '';
      send.textContent = replaces ? t('standby.add.replace') : t('standby.add.send');
      send.className = replaces ? 'danger-button' : 'primary-button';
      send.disabled = Boolean(problem);
    }
    input.addEventListener('input', check);
    input.addEventListener('keydown', (evt) => {
      if (evt.key !== 'Enter') return;
      evt.preventDefault();
      // The destructive answer is never the default: Replace takes its own click.
      if (send.disabled || replacing) return;
      send.click();
    });
    check();
    void preview();
    fits.find((b) => b.dataset.fit === fit).focus();
    return (await m.result) === 'ok' ? sent : null;
  }

  // ------------------------------------------------------------ public --
  return {
    /** The Settings subtab of the Screen tab is shown: read the screen's choice. */
    show() {
      view.shown = true;
      void load();
    },
    hide() {
      view.shown = false;
      popovers.close();
    },
    /** The screen chosen at the top, or its state, changed. */
    update() {
      const current = screen();
      const signature = JSON.stringify([current?.key ?? null, current?.state ?? null, current?.family ?? null]);
      if (signature === view.signature) return;
      const other = (current?.key ?? null) !== view.key;
      view.signature = signature;
      view.key = current?.key ?? null;
      if (other) Object.assign(view, { data: null, error: null, problem: null, status: 'idle' });
      if (view.shown && current) void load();
      else render();
    },
    /** The UI's language changed: every text is drawn again. */
    retranslate() {
      render();
    },
  };
}
