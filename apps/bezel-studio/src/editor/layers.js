import { withChildren } from './cards.js';
// Presentation only: persistent paint order remains a flat array.
export function layerGroups(elements) {
  const nodes = new Map(elements.map(element => [element.id, { element, groups: element.card ?
    [null, ...element.card.faces.map((_, i) => i)].map(face => ({ face, elements: [], nodes: [] })) : [], children: [] }]));
  const roots = [];
  for (const e of [...elements].reverse()) {
    const node = nodes.get(e.id), group = nodes.get(e.groupParent);
    if (group?.element.isGroup && group !== node) { group.children.push(node); continue; }
    const card = nodes.get(e.cardMember?.parent);
    const face = card?.groups.find(g => g.face === e.cardMember.face);
    if (!e.card && face) { face.elements.push(e); face.nodes.push(node); }
    else roots.push(node);
  }
  return roots;
}

/** Drop above/below a displayed sibling; return the post-removal paint index. */
export function layerDropIndex(elements, id, targetId, before) {
  if (id === targetId) return null;
  const roots = layerGroups(elements);
  const lists = [];
  const visit = nodes => {
    lists.push(nodes.map(n => n.element));
    for (const node of nodes) {
      if (node.children.length) visit(node.children);
      for (const face of node.groups) visit(face.nodes);
    }
  };
  visit(roots);
  const siblings = lists.find(group => group.some(e => e.id === id));
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
