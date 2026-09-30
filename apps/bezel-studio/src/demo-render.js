// An approximate renderer for demo mode (browser only). The real preview is
// the Rust renderer's frame; this only has to look plausible and be fast.

function color(hex) {
  if (typeof hex !== 'string' || !hex.startsWith('#')) return 'transparent';
  const h = hex.slice(1);
  const n = (i) => parseInt(h.slice(i, i + 2), 16);
  const a = h.length === 8 ? n(6) / 255 : 1;
  return `rgba(${n(0)},${n(2)},${n(4)},${a})`;
}

function paint(p) {
  if (typeof p === 'string') return color(p);
  if (p && p.stops) return color(p.stops[0]?.[1]);
  return 'transparent';
}

function fraction(t, i) {
  return 0.5 + 0.4 * Math.sin(t / 3 + i);
}

function drawText(ctx, e, t) {
  const k = e.kind;
  const s = k.style;
  let text = k.content.text ?? '';
  if (k.content.type === 'clock') {
    const d = new Date(t * 1000);
    text = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  } else if (k.content.type === 'sensor') {
    text = `${k.content.prefix ?? ''}${Math.round(fraction(t, e.id) * 100)}${k.content.format?.showUnit === false ? '' : '%'}${k.content.suffix ?? ''}`;
  }
  ctx.fillStyle = paint(s.paint);
  ctx.font = `${s.font?.italic ? 'italic ' : ''}${s.font?.weight ?? 400} ${s.size}px ${s.font?.family ?? 'sans-serif'}, sans-serif`;
  ctx.textAlign = s.align === 'center' ? 'center' : s.align === 'right' ? 'right' : 'left';
  ctx.textBaseline = s.valign === 'top' ? 'top' : s.valign === 'bottom' ? 'bottom' : 'middle';
  const f = e.frame;
  const x = s.align === 'center' ? f.x + f.width / 2 : s.align === 'right' ? f.x + f.width : f.x;
  const y = s.valign === 'top' ? f.y : s.valign === 'bottom' ? f.y + f.height : f.y + f.height / 2;
  ctx.fillText(text, x, y, f.width);
}

function drawElement(ctx, e, t) {
  const k = e.kind;
  const f = e.frame;
  const frac = fraction(t, e.id);
  ctx.save();
  ctx.globalAlpha = e.opacity ?? 1;
  switch (k.type) {
    case 'text':
      drawText(ctx, e, t);
      break;
    case 'shape':
      ctx.fillStyle = paint(k.fill);
      ctx.beginPath();
      if (k.shape === 'ellipse') ctx.ellipse(f.x + f.width / 2, f.y + f.height / 2, f.width / 2, f.height / 2, 0, 0, Math.PI * 2);
      else ctx.roundRect(f.x, f.y, f.width, f.height, k.radius ?? 0);
      ctx.fill();
      break;
    case 'bar': {
      ctx.fillStyle = paint(k.track);
      ctx.fillRect(f.x, f.y, f.width, f.height);
      ctx.fillStyle = paint(k.fill);
      const vertical = k.direction === 'bottomToTop' || k.direction === 'topToBottom';
      if (vertical) ctx.fillRect(f.x, f.y + f.height * (1 - frac), f.width, f.height * frac);
      else ctx.fillRect(f.x, f.y, f.width * frac, f.height);
      break;
    }
    case 'ring': {
      const r = Math.min(f.width, f.height) / 2 - k.thickness / 2;
      const cx = f.x + f.width / 2;
      const cy = f.y + f.height / 2;
      const start = ((k.startAngle - 90) * Math.PI) / 180;
      const sweep = (k.sweep * Math.PI) / 180;
      ctx.lineWidth = k.thickness;
      ctx.lineCap = k.roundCaps ? 'round' : 'butt';
      ctx.strokeStyle = paint(k.track);
      ctx.beginPath();
      ctx.arc(cx, cy, r, start, start + sweep);
      ctx.stroke();
      ctx.strokeStyle = paint(k.fill);
      ctx.beginPath();
      ctx.arc(cx, cy, r, start, start + sweep * frac);
      ctx.stroke();
      break;
    }
    case 'needle': {
      const px = f.x + f.width * (k.pivot?.[0] ?? 0.5);
      const py = f.y + f.height * (k.pivot?.[1] ?? 0.5);
      const angle = (((k.startAngle + k.sweep * frac) - 90) * Math.PI) / 180;
      const len = Math.min(f.width, f.height) * 0.45;
      ctx.strokeStyle = color(k.color);
      ctx.lineWidth = k.width;
      ctx.lineCap = 'round';
      ctx.beginPath();
      ctx.moveTo(px, py);
      ctx.lineTo(px + Math.cos(angle) * len, py + Math.sin(angle) * len);
      ctx.stroke();
      break;
    }
    case 'graph': {
      ctx.strokeStyle = color(k.color);
      ctx.lineWidth = k.lineWidth;
      ctx.beginPath();
      const n = 30;
      for (let i = 0; i < n; i += 1) {
        const x = f.x + (f.width * i) / (n - 1);
        const y = f.y + f.height * (1 - fraction(t - (n - i), e.id));
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
      break;
    }
    case 'image':
      ctx.strokeStyle = 'rgba(255,255,255,0.4)';
      ctx.setLineDash([6, 6]);
      ctx.strokeRect(f.x, f.y, f.width, f.height);
      break;
    default:
      break;
  }
  ctx.restore();
}

/**
 * Renders a theme to RGBA pixels.
 * @returns {{width:number, height:number, rgba: Uint8ClampedArray}}
 */
export function renderApprox(theme, t) {
  const { width, height } = theme.canvas;
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext('2d');
  ctx.fillStyle = theme.background?.type === 'color' ? color(theme.background.color) : '#000';
  ctx.fillRect(0, 0, width, height);
  for (const e of theme.elements) if (e.visible !== false) drawElement(ctx, e, t);
  return { width, height, rgba: ctx.getImageData(0, 0, width, height).data };
}
