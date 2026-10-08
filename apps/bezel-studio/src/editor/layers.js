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
