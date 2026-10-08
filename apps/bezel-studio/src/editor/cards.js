/** Cards keep ordinary objects in the theme, with stable IDs and ownership. */
export function isShown(theme, element) {
  if (element.visible === false) return false;
  if (!element.cardMember) return true;
  const parent = theme.elements.find(e => e.id === element.cardMember.parent);
  return Boolean(parent?.card && parent.visible !== false &&
    (element.cardMember.face == null || element.cardMember.face === parent.card.activeFace));
}

export function withChildren(theme, ids) {
  const result = new Set(ids);
  for (const e of theme.elements) if (result.has(e.cardMember?.parent)) result.add(e.id);
  return [...result];
}

export function transformCard(theme, id, frame) {
  const parent = theme.elements.find(e => e.id === id);
  const old = parent.frame;
  const sx = frame.width / old.width, sy = frame.height / old.height;
  return { ...theme, elements: theme.elements.map(e => {
    if (e.id === id) return { ...e, frame };
    if (parent.card && e.cardMember?.parent === id) return { ...e, frame: {
      x: frame.x + (e.frame.x - old.x) * sx, y: frame.y + (e.frame.y - old.y) * sy,
      width: Math.max(4, e.frame.width * sx), height: Math.max(4, e.frame.height * sy),
    }};
    return e;
  }) };
}
