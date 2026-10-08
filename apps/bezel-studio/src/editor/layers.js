import { withChildren } from './cards.js';
// Presentation only: keep the theme's flat paint order and membership intact.
export function layerGroups(elements) {
  const cards = new Map(elements.filter(e => e.card).map(e => [e.id, e]));
  const grouped = new Set();
  const children = new Map();
  for (const e of elements) {
    const member = e.cardMember, parent = cards.get(member?.parent);
    if (e.card || !parent || !(member.face === null || Number.isInteger(member.face) && member.face >= 0 && member.face < parent.card.faces.length)) continue;
    grouped.add(e.id);
    if (!children.has(parent.id)) children.set(parent.id, []);
    children.get(parent.id).push(e);
  }
  return elements.filter(e => !grouped.has(e.id)).reverse().map(element => ({
    element,
    groups: element.card ? [null, ...element.card.faces.map((_, i) => i)].map(face => ({
      face,
      elements: (children.get(element.id) ?? []).filter(e => e.cardMember.face === face).reverse(),
    })) : [],
  }));
}

/** Drop above/below a displayed sibling; return the post-removal paint index. */
export function layerDropIndex(elements, id, targetId, before) {
  if (id === targetId) return null;
  const roots = layerGroups(elements);
  const siblings = [roots.map(g => g.element), ...roots.flatMap(g => g.groups.map(face => face.elements))]
    .find(group => group.some(e => e.id === id));
  if (!siblings?.some(e => e.id === targetId)) return null;
  const theme = { elements };
  const moved = new Set(withChildren(theme, [id]));
  const anchors = new Set(withChildren(theme, [targetId]));
  const remaining = elements.filter(e => !moved.has(e.id));
  const indices = remaining.flatMap((e, i) => anchors.has(e.id) ? [i] : []);
  if (!indices.length) return null;
  // The list shows topmost first, opposite to the theme's paint order.
  return before ? Math.max(...indices) + 1 : Math.min(...indices);
}
