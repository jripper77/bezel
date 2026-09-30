// DOM glue: wires the bridge, the view model and the page together.
import { applyTranslations, pickLocale, translator } from './i18n/index.js';
import { createBridge } from './bridge.js';
import { countLabel, detailRows, fitPreview, screenCard, soleModel } from './view-model.js';

const locale = pickLocale(navigator.languages ?? [navigator.language]);
const t = translator(locale);
const bridge = createBridge(window);
const $ = (id) => document.getElementById(id);

const state = { screens: [], selected: null };

function renderList() {
  const list = $('screen-list');
  list.replaceChildren(
    ...state.screens.map((screen) => {
      const card = screenCard(screen, t);
      const item = document.createElement('li');
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'screen-card';
      button.dataset.key = card.key;
      button.setAttribute('aria-pressed', String(card.key === state.selected));
      button.addEventListener('click', () => select(card.key));
      const title = Object.assign(document.createElement('span'), { className: 'screen-card-title', textContent: card.title });
      const meta = Object.assign(document.createElement('span'), { className: 'screen-card-meta', textContent: card.meta });
      const badge = Object.assign(document.createElement('span'), { className: `badge ${card.state}`, textContent: card.stateLabel });
      button.append(title, meta, badge);
      item.append(button);
      return item;
    }),
  );
  $('empty').hidden = state.screens.length > 0;
}

function renderSelection() {
  const screen = state.screens.find((s) => s.key === state.selected);
  const frame = $('device-frame');
  const model = screen ? soleModel(screen) : null;
  frame.hidden = !model;
  $('stage-hint').hidden = !screen;
  if (model) {
    const stage = frame.parentElement.getBoundingClientRect();
    const size = fitPreview(model, { width: stage.width * 0.8, height: stage.height * 0.72 });
    Object.assign($('device-screen').style, { width: `${size.width}px`, height: `${size.height}px` });
    $('device-size').textContent = `${model.width}×${model.height}`;
  }
  const details = $('details');
  details.replaceChildren(
    ...(screen ? detailRows(screen, t) : []).flatMap(({ label, value }) => [
      Object.assign(document.createElement('dt'), { textContent: label }),
      Object.assign(document.createElement('dd'), { textContent: value }),
    ]),
  );
}

// Only the pressed state changes, so keyboard focus stays on the card.
function select(key) {
  state.selected = key;
  for (const button of $('screen-list').querySelectorAll('button')) {
    button.setAttribute('aria-pressed', String(button.dataset.key === key));
  }
  renderSelection();
}

async function refresh() {
  $('status').textContent = t('screens.loading');
  try {
    state.screens = await bridge.listScreens();
    if (!state.screens.some((s) => s.key === state.selected)) state.selected = state.screens[0]?.key ?? null;
    $('status').textContent = countLabel(state.screens.length, t);
  } catch (error) {
    state.screens = [];
    state.selected = null;
    $('status').textContent = t('screens.error', { message: error?.message ?? String(error) });
  }
  renderList();
  renderSelection();
}

document.documentElement.lang = locale;
applyTranslations(document, t);
$('refresh').addEventListener('click', refresh);
window.addEventListener('resize', renderSelection);
refresh();
