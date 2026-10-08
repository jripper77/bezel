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

/** Validate a layer reparent/reorder without changing geometry or IDs. */
export function layerPlacement(theme,{ids,id,target,inside=false,face,root=false,before=true}) {
  const roots=theme.elements.filter(e=>(ids ?? [id]).includes(e.id) && !withChildren(theme,(ids ?? [id]).filter(n=>n!==e.id)).includes(e.id));
  if(!roots.length)return null;
  const moved=new Set(withChildren(theme,roots.map(e=>e.id)));
  const owner=theme.elements.find(e=>e.id===target);
  if(!root && (!owner || moved.has(target)))return null;
  let groupParent=null,cardMember=null;
  if(!root) {
    if(inside && owner.isGroup){groupParent=owner.id;cardMember=owner.cardMember ?? null;}
    else if(inside && owner.card){if(face!==null && face!==undefined && (!Number.isInteger(face)||face<0||face>=owner.card.faces.length))return null;cardMember={parent:owner.id,face:face===undefined?owner.card.activeFace:face};}
    else if(inside)return null;
    else {groupParent=owner.groupParent ?? null;cardMember=owner.cardMember ?? null;}
  }
  const locked=e=>e?.locked || (e && (e.groupParent!=null || e.cardMember) && theme.elements.some(p=>(p.id===e.groupParent || p.id===e.cardMember?.parent)&&locked(p)));
  const changed=roots.some(e=>(e.groupParent ?? null)!==groupParent || (e.cardMember?.parent ?? null)!==(cardMember?.parent ?? null) || (e.cardMember?.face ?? null)!==(cardMember?.face ?? null));
  const container=theme.elements.find(e=>e.id===(groupParent ?? cardMember?.parent));
  if(changed && (roots.some(locked) || locked(container)))return null;
  if(cardMember && theme.elements.some(e=>moved.has(e.id)&&e.card))return null;
  return {roots:roots.map(e=>e.id),moved,owner,groupParent,cardMember,inside,root,before,changed};
}
