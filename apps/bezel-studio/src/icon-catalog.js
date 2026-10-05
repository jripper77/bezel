// Offline Tabler catalog. SVGs and their MIT notice ship with Studio.
const ALIASES = {
  ventola: ['propeller', 'windmill', 'fan'], fan: ['propeller', 'windmill', 'fan'],
  temperatura: ['temperature', 'thermometer'], gradi: ['temperature', 'thermometer'],
  processore: ['cpu'], memoria: ['device-sd-card', 'database'], ram: ['device-sd-card', 'database'],
  rete: ['network', 'wifi', 'ethernet'], disco: ['device-harddisk', 'database'],
  schermo: ['device-desktop', 'device-monitor'], gpu: ['device-desktop', 'cpu'],
  fulmine: ['bolt', 'lightning-bolt', 'flash'], fulmini: ['bolt', 'lightning-bolt', 'flash'],
  elettricita: ['bolt', 'lightning-bolt', 'flash', 'power', 'electric'], elettrico: ['bolt', 'lightning-bolt', 'flash', 'power', 'electric'],
  pompa: ['water-pump'], pompe: ['water-pump'], pump: ['water-pump'],
  raffreddamento: ['water-pump', 'radiator', 'fan', 'propeller', 'coolant', 'snowflake'],
  liquido: ['water-pump', 'droplet', 'water', 'coolant'], acqua: ['water-pump', 'droplet', 'water'],
  radiatore: ['radiator'], tubi: ['pipe', 'pipeline'], circuito: ['pipe', 'pipeline', 'network'],
  valvola: ['valve'], flusso: ['water-pump', 'pipe', 'gauge'],
  energia: ['bolt', 'power', 'battery'], potenza: ['bolt', 'power'],
  luce: ['bulb', 'sun'], led: ['bulb', 'rgb'], orologio: ['clock'],
};

/** Searches every icon, accepting hardware terms in Italian as well. */
export function filterIcons(icons, query = '', style = 'all', source = 'all') {
  const stop = new Set(['di', 'del', 'della', 'da', 'per', 'la', 'il', 'lo', 'le', 'gli', 'un', 'una', 'the', 'of', 'for']);
  const words = query.normalize('NFD').replace(/\p{M}/gu, '').trim().toLowerCase().split(/[^a-z0-9-]+/).filter((word) => word && !stop.has(word));
  const found = icons.filter((item) => (style === 'all' || item.style === style)
    && (source === 'all' || (item.provider ?? 'tabler') === source)
    && words.every((word) => (ALIASES[word] ?? [word]).some((term) => item.id.includes(term))));
  // Primary symbols precede decorated variants, e.g. bolt before calendar-bolt.
  const score = (item) => words.reduce((sum, word) => {
    const id = item.id.replace(/^mdi-/, '').replace(/-filled$/, '');
    const terms = ALIASES[word] ?? [word];
    return sum + Math.min(...terms.map((term) => id === term ? 0 : id.startsWith(term) ? 1 : 2));
  }, 0);
  return words.length ? found.sort((a, b) => score(a) - score(b)) : found;
}

let catalogPromise;
/** Both licensed collections are embedded; no runtime downloads. */
export function loadIconCatalog() {
  catalogPromise ??= Promise.all([import('./assets/tabler/icons.js'), import('./assets/mdi/icons.js')])
    .then(([tabler, mdi]) => ({ icons: [...tabler.default.icons, ...mdi.default.icons] }))
    .catch((error) => { catalogPromise = null; throw error; });
  return catalogPromise;
}

/** Paint and effects stay in the SVG, including the editable values. */
export function iconSvg(item, color = '#ffffff', stroke = 2, { shadow = 0, shadowColor = '#00000080' } = {}) {
  if (!/^#[\da-f]{6}([\da-f]{2})?$/i.test(color) || !/^#[\da-f]{6}([\da-f]{2})?$/i.test(shadowColor)) throw new Error('invalidIconColor');
  const width = Number(stroke);
  const blur = Number(shadow);
  if (!Number.isFinite(width) || width < 0.5 || width > 4) throw new Error('invalidIconStroke');
  if (!Number.isFinite(blur) || blur < 0 || blur > 4) throw new Error('invalidIconShadow');
  const alpha = (hex) => hex.length === 9 ? parseInt(hex.slice(7), 16) / 255 : 1;
  let svg = item.svg.replaceAll('currentColor', color.slice(0, 7)).replace('stroke-width="2"', `stroke-width="${width}"`);
  svg = svg.replace(/<svg\s/, `<svg data-bezel-icon="${item.id}" data-bezel-color="${color}" data-bezel-stroke="${width}" data-bezel-shadow="${blur}" data-bezel-shadow-color="${shadowColor}" fill-opacity="${alpha(color)}" stroke-opacity="${alpha(color)}" `);
  if (blur > 0) {
    const pad = Math.ceil(blur * 3 + 2);
    svg = svg.replace('viewBox="0 0 24 24"', `viewBox="${-pad} ${-pad} ${24 + pad * 2} ${24 + pad * 2}"`);
    const end = svg.indexOf('>') + 1;
    const filter = `<defs><filter id="bezel-shadow" x="-100%" y="-100%" width="300%" height="300%" color-interpolation-filters="sRGB"><feDropShadow` + ` dx="0" dy="1" stdDeviation="${blur}" flood-color="${shadowColor.slice(0, 7)}" flood-opacity="${alpha(shadowColor)}"/></filter></defs><g filter="url(#bezel-shadow)">`;
    svg = svg.slice(0, end) + filter + svg.slice(end).replace('</svg>', '</g></svg>');
  }
  return svg;
}

/** Recognise saved Tabler SVGs, including icons added before editable effects. */
export function readIcon(svg) {
  const root = typeof svg === 'string' ? svg.match(/<svg\s[^>]*>/)?.[0] : null;
  if (!root) return null;
  const attr = (name) => root.match(new RegExp(`${name}="([^"<>]*)"`))?.[1];
  const cls = attr('class') ?? '';
  let id = attr('data-bezel-icon') ?? cls.match(/(?:^|\s)icon-tabler-([a-z0-9-]+)(?:\s|$)/)?.[1];
  if (!id || !/^[a-z0-9-]+$/.test(id)) return null;
  const style = cls.includes('icons-tabler-filled') || cls.includes('icons-mdi-filled') ? 'filled' : 'outline';
  if (style === 'filled' && !attr('data-bezel-icon')) id += '-filled';
  const color = attr('data-bezel-color') ?? attr(style === 'filled' ? 'fill' : 'stroke');
  const stroke = Number(attr('data-bezel-stroke') ?? attr('stroke-width') ?? 2);
  const shadow = Number(attr('data-bezel-shadow') ?? 0);
  const shadowColor = attr('data-bezel-shadow-color') ?? '#00000080';
  if (!/^#[\da-f]{6}([\da-f]{2})?$/i.test(color ?? '') || !/^#[\da-f]{6}([\da-f]{2})?$/i.test(shadowColor)
    || !Number.isFinite(stroke) || stroke < 0.5 || stroke > 4 || !Number.isFinite(shadow) || shadow < 0 || shadow > 4) return null;
  return { id, style, color, stroke, shadow, shadowColor };
}

/** Preview as an image rather than executable inline markup. */
export function iconUrl(svg) {
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}
