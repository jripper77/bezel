// Transient animation state: never part of a saved theme or Undo snapshot.
export function createCardMotion() {
  const states = new Map();
  let last = 0;
  return {
    update(theme, now, motion = true) {
      if (now < last) states.clear();
      last = now;
      const result = new Map();
      for (const id of states.keys()) if (!theme.elements.some(e => e.id === id && e.card)) states.delete(id);
      for (const e of theme.elements.filter(e => e.card)) {
        const c = e.card, settings = c.transition;
        const signature = JSON.stringify(c.faces), configuration = JSON.stringify(settings);
        let state = states.get(e.id);
        if (!state || state.signature !== signature || e.visible === false || !motion) state = { signature, configuration, active: c.activeFace, from: c.activeFace, started: now };
        if (state.configuration !== configuration) { state.from = state.active; state.configuration = configuration; }
        if (state.active !== c.activeFace) {
          state.from = settings?.effect === 'flip' && now - state.started < settings.durationMs / 2 ? state.from : state.active;
          state.active = c.activeFace; state.started = now;
        }
        states.set(e.id, state);
        if (settings && settings.effect !== 'none' && state.from !== state.active && now - state.started < settings.durationMs) {
          result.set(e.id, { from: state.from, to: state.active, progress: Math.max(0, (now - state.started) / settings.durationMs), settings });
        } else state.from = state.active;
      }
      return result;
    },
  };
}

export function cardPoses(transition, b) {
  const { from, to, settings } = transition;
  const raw = Math.max(0, Math.min(1, transition.progress)), p = raw * raw * (3 - 2 * raw);
  const horizontal = ['left', 'right'].includes(settings.direction), sign = ['left', 'up'].includes(settings.direction) ? -1 : 1;
  const pose = (face, alpha = 1) => ({ face, alpha, shade: 1, matrix: [1, 0, 0, 1, 0, 0] });
  if (settings.effect === 'fade') return [pose(from, 1 - p), pose(to, p)];
  if (settings.effect === 'slide') return [[from, p], [to, p - 1]].map(([face, travel]) => ({ ...pose(face), matrix: [1, 0, 0, 1, horizontal ? sign * travel * b.width : 0, horizontal ? 0 : sign * travel * b.height] }));
  if (settings.effect === 'flip') {
    const scale = Math.abs(Math.cos(Math.PI * p)), cx = b.x + b.width / 2, cy = b.y + b.height / 2;
    const depth = sign * Math.sin(Math.PI * p) * 0.22 * (p < 0.5 ? 1 : -1);
    return [{ ...pose(p < 0.5 ? from : to, scale < 0.002 ? 0 : 1), shade: 0.84 + scale * 0.16,
      projection: { horizontal, cx, cy, half: (horizontal ? b.width : b.height) / 2, scale, depth },
      matrix: horizontal ? [Math.max(0.001, scale), 0, 0, 1, cx * (1 - scale), 0] : [1, 0, 0, Math.max(0.001, scale), 0, cy * (1 - scale)] }];
  }
  return [pose(to)];
}

// Piecewise projective texture mapping for the browser demo. The native
// renderer uses the same projection with inverse bilinear pixel sampling.
export function drawCardProjection(ctx, image, projection) {
  const { horizontal, cx, cy, half, scale, depth } = projection;
  if (scale < 0.002) return;
  const extent = horizontal ? image.width : image.height;
  for (let start = 0; start < extent; start += 2) {
    const end = Math.min(extent, start + 2), center = horizontal ? cx : cy;
    const d0 = 1 + depth * (start - center) / Math.max(1, half);
    const d1 = 1 + depth * (end - center) / Math.max(1, half);
    if (d0 <= 0 || d1 <= 0) continue;
    const at = center + scale * (start - center) / d0;
    const next = center + scale * (end - center) / d1;
    const d = (d0 + d1) / 2;
    if (horizontal) ctx.drawImage(image, start, 0, end - start, image.height, at, cy - cy / d, next - at, image.height / d);
    else ctx.drawImage(image, 0, start, image.width, end - start, cx - cx / d, at, image.width / d, next - at);
  }
}
