// "Connect your smart screen" (artboard Connect): what the stage shows while
// no screen can be used, because none is listed or the system denied the
// selected one's port. It lists only what the bridge reports (screens the
// system refused, panels left in desktop mode, a failed enumeration), shows
// the udev command a denied port needs (ui/udev.js) and imports a theme
// (bridge.importTheme). It covers the canvas, so it can be hidden; it comes
// back when what it says changes.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { udevCommand } from './udev.js';

/** Tips that hold for every screen, in order. */
const STEPS = ['cable', 'vendor', 'permissions'];

/**
 * @param {{
 *   t: (k: string, p?: object) => string,
 *   notify: (message: string) => void,
 *   errorText: (t: Function, e: any) => string,
 *   deviceLabel: (screen: object) => string,
 *   onImport: () => void,
 *   onOpenScreen: () => void,
 * }} deps
 */
export function createConnect({ t, notify, errorText, deviceLabel, onImport, onOpenScreen }) {
  const root = document.getElementById('connect');
  let view = { screens: [], desktopMode: [], screenError: null, denied: null };
  let hiddenFor = null;

  /** What the card says, as a key: hiding it lasts until this changes. */
  const signature = () => JSON.stringify([
    view.screens.length,
    view.desktopMode.map((p) => p.key),
    view.screenError ? errorText(t, view.screenError) : null,
    view.denied?.key ?? null,
  ]);

  const wanted = () => !view.screens.length || Boolean(view.denied);

  function glyph(model) {
    const portrait = model ? model.height >= model.width : true;
    return el('span', { class: 'connect-glyph', dataset: { shape: portrait ? 'portrait' : 'landscape' }, 'aria-hidden': 'true' });
  }

  function deniedRow({ screen, error }) {
    return el('li', { class: 'connect-device', dataset: { state: 'denied' } }, [
      glyph(screen?.models?.[0]),
      el('div', { class: 'connect-device-text' }, [
        el('strong', { text: screen ? deviceLabel(screen) : error.args?.address ?? '' }),
        el('span', { class: 'connect-device-why', text: errorText(t, error) }),
        error.udevCommand ? el('p', { class: 'hint', text: t('udev.explain') }) : null,
        error.udevCommand ? udevCommand(t, error.udevCommand, notify) : null,
        error.udevCommand ? el('p', { class: 'hint', text: t('udev.never') }) : null,
      ]),
    ]);
  }

  function desktopRow(panel) {
    return el('li', { class: 'connect-device', dataset: { state: 'desktop' } }, [
      glyph(panel.models?.[0]),
      el('div', { class: 'connect-device-text' }, [
        el('strong', { text: t('desktop.name') }),
        el('span', { class: 'connect-device-meta', text: `${panel.key} · ${panel.usb}` }),
        el('span', { class: 'connect-device-why', text: t('desktop.hint') }),
      ]),
      el('button', { type: 'button', class: 'text-button', text: t('connect.openScreen'), onclick: onOpenScreen }),
    ]);
  }

  function devices() {
    const rows = [
      ...(view.denied ? [deniedRow(view.denied)] : []),
      ...view.desktopMode.map(desktopRow),
    ];
    let status = 'searching';
    if (view.screenError) status = 'error';
    else if (rows.length) status = 'found';
    return el('section', { class: 'connect-devices', 'aria-labelledby': 'connect-devices-title', dataset: { state: status } }, [
      el('div', { class: 'connect-devices-head' }, [
        icon(status === 'error' ? ICONS.warning : ICONS.refresh, 18),
        el('h3', { id: 'connect-devices-title', text: t(`connect.devices.${status}`) }),
        el('span', { class: 'connect-buses', text: t('connect.buses') }),
      ]),
      view.screenError
        ? el('p', { class: 'connect-device-why', role: 'alert', text: t('status.devicesError', { message: errorText(t, view.screenError) }) })
        : null,
      rows.length ? el('ul', { class: 'connect-device-list' }, rows) : el('p', { class: 'hint', text: t('connect.devices.every') }),
    ]);
  }

  function steps() {
    return el('ol', { class: 'connect-steps' }, STEPS.map((step, i) => el('li', {}, [
      el('span', { class: 'connect-step-number', 'aria-hidden': 'true', text: String(i + 1) }),
      el('strong', { text: t(`connect.step.${step}`) }),
      el('span', { text: t(`connect.step.${step}Hint`) }),
    ])));
  }

  function render() {
    const hide = el('button', {
      type: 'button',
      class: 'icon-button connect-hide',
      title: t('connect.hide'),
      'aria-label': t('connect.hide'),
      onclick: () => {
        hiddenFor = signature();
        root.hidden = true;
        // The focus stays in the stage, on the canvas's zoom.
        document.getElementById('zoom-fit')?.focus();
      },
    }, [icon(ICONS.close, 16)]);
    root.replaceChildren(el('div', { class: 'connect-card' }, [
      el('div', { class: 'connect-head' }, [
        el('h2', { id: 'connect-title', text: t('connect.title') }),
        hide,
      ]),
      el('p', { class: 'connect-lead', text: t('connect.lead') }),
      devices(),
      steps(),
      el('div', { class: 'connect-import' }, [
        el('button', { type: 'button', class: 'text-button', id: 'connect-import', onclick: onImport }, [icon(ICONS.upload, 16), t('connect.import')]),
        el('span', { class: 'hint', text: t('connect.importHint') }),
      ]),
    ]));
  }

  return {
    /**
     * @param {{screens: object[], desktopMode?: object[], screenError?: any, denied?: {key: string, screen: object|null, error: any}|null}} next
     */
    update(next) {
      view = { screens: next.screens ?? [], desktopMode: next.desktopMode ?? [], screenError: next.screenError ?? null, denied: next.denied ?? null };
      const show = wanted() && hiddenFor !== signature();
      if (!show) {
        root.hidden = true;
        return;
      }
      // Rebuilt only when what it says changes, so a focused button keeps its focus.
      const key = `${signature()}|${document.documentElement.lang}`;
      if (root.dataset.shown !== key) {
        root.dataset.shown = key;
        render();
      }
      root.hidden = false;
    },
  };
}
