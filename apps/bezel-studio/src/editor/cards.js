import { unionBox } from './geometry.js';

// Follow both ownership relations, counting a shared card ancestor once.
export function owners(theme, element) {
  const result = [element], seen = new Set([element.id]);
  for (let i = 0; i < result.length; i++) {
    for (const id of [result[i].groupParent, result[i].cardMember?.parent]) {
      if (id == null || seen.has(id)) continue;
      const parent = theme.elements.find(e => e.id === id);
      if (!parent) return [];
      seen.add(id); result.push(parent);
    }
  }
  return result;
}

export function visibleWithoutFace(theme, element) {
  const all = owners(theme, element);
  return all.length > 0 && all.every(e => e.visible !== false);
}

export function effectiveOpacity(theme, element) {
  return owners(theme, element).reduce((alpha, e) => alpha * (e.opacity ?? 1), 1);
}

export function isShown(theme, element) {
  const all = owners(theme, element);
  return all.length > 0 && all.every(e => e.visible !== false && (!e.cardMember ||
    theme.elements.some(p => p.id === e.cardMember.parent && p.card &&
      (e.cardMember.face == null || e.cardMember.face === p.card.activeFace))));
}

export function withChildren(theme, ids) {
  const result = new Set(ids);
  let changed;
  do {
    changed = false;
    for (const e of theme.elements) if (!result.has(e.id) &&
      (result.has(e.groupParent) || result.has(e.cardMember?.parent))) {
      result.add(e.id); changed = true;
    }
  } while (changed);
  return [...result];
}

export function selectionRoots(theme, ids) {
  const selected = new Set(ids);
  return theme.elements.filter(e => selected.has(e.id) &&
    !owners(theme, e).slice(1).some(p => selected.has(p.id)));
}

export function canGroup(theme, ids) {
  const members = selectionRoots(theme, ids).filter(e => !owners(theme, e).some(p => p.locked));
  if (members.length < 2) return false;
  const first = members[0];
  return members.every(e => (e.groupParent ?? null) === (first.groupParent ?? null) &&
    (e.cardMember?.parent ?? null) === (first.cardMember?.parent ?? null) &&
    (e.cardMember?.face ?? null) === (first.cardMember?.face ?? null));
}

// Clicking a member normally selects its outermost ordinary group. Layers
// and an explicit Ctrl-click still allow editing an individual member.
export function groupTarget(theme, id) {
  let element = theme.elements.find(e => e.id === id);
  const seen = new Set();
  while (element?.groupParent != null && !seen.has(element.id)) {
    seen.add(element.id);
    element = theme.elements.find(e => e.id === element.groupParent);
  }
  return element?.id ?? id;
}

export function transformCard(theme, id, frame) {
  const parent = theme.elements.find(e => e.id === id);
  const old = parent.frame;
  const sx = frame.width / old.width, sy = frame.height / old.height;
  const children = new Set(parent.card || parent.isGroup ? withChildren(theme, [id]) : []);
  return { ...theme, elements: theme.elements.map(e => {
    if (e.id === id) return { ...e, frame };
    if (children.has(e.id)) return { ...e, frame: {
      x: frame.x + (e.frame.x - old.x) * sx, y: frame.y + (e.frame.y - old.y) * sy,
      width: Math.max(4, e.frame.width * sx), height: Math.max(4, e.frame.height * sy),
    }};
    return e;
  }) };
}

// Group bounds follow edits to individual members, deepest groups first.
export function syncGroupBounds(theme) {
  if (!theme.elements.some(e => e.isGroup)) return theme;
  let elements = theme.elements;
  const groups = elements.filter(e => e.isGroup).sort((a, b) =>
    owners(theme, b).length - owners(theme, a).length);
  for (const group of groups) {
    const members = elements.filter(e => e.groupParent === group.id);
    if (!members.length) { elements = elements.filter(e => e.id !== group.id); continue; }
    const frame = unionBox(members.map(e => e.frame));
    if (Object.keys(frame).some(k => frame[k] !== group.frame[k]))
      elements = elements.map(e => e.id === group.id ? { ...e, frame } : e);
  }
  return elements === theme.elements ? theme : { ...theme, elements };
}
