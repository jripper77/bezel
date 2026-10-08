// Demo playback mirrors the native runtime, outside the document/store.
export function createCardTimers() {
  const states = new Map();
  let last = 0;
  return {
    update(theme, now, enabled = true) {
      if (!enabled || now < last) states.clear();
      last = now;
      let due = null;
      const valid = theme.elements.filter(e => enabled && e.visible !== false && e.card?.faces.length > 1 && Number.isInteger(e.card.rotationSeconds) && e.card.rotationSeconds >= 5 && e.card.rotationSeconds <= 3600);
      const ids = new Set(valid.map(e => e.id));
      for (const id of states.keys()) if (!ids.has(id)) states.delete(id);
      for (const e of valid) {
        const signature = JSON.stringify([e.card.faces, e.card.activeFace, e.card.rotationSeconds]);
        let state = states.get(e.id);
        const interval = e.card.rotationSeconds * 1000;
        if (!state || state.signature !== signature) state = { signature, face: e.card.activeFace, due: now + interval };
        if (now >= state.due) { state.face = (state.face + 1) % e.card.faces.length; state.due = now + interval; }
        states.set(e.id, state);
        due = due === null ? state.due : Math.min(due, state.due);
      }
      const elements = theme.elements.map(e => {
        const face = states.get(e.id)?.face;
        return face == null || face === e.card?.activeFace ? e : { ...e, card: { ...e.card, activeFace: face } };
      });
      return { theme: elements.some((e, i) => e !== theme.elements[i]) ? { ...theme, elements } : theme, nextMs: due === null ? null : Math.max(0, due - now) };
    },
  };
}
