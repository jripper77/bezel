import { weatherText } from './weather-format.js';
import { formatClock } from './clock-format.js';
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

/** How long each frame of the demo's animated GIFs (every `*.gif` image) lasts, ms. */
export const DEMO_GIF_FRAME_MS = 100;
/** The colors the demo's GIFs cycle through, one per frame. */
const GIF_COLORS = ['#e4572e', '#f3a712', '#29bf12', '#4361ee'];

function fraction(t, i) {
  return 0.5 + 0.4 * Math.sin(t / 3 + i);
}

function drawText(ctx, e, t) {
  const k = e.kind;
  const s = k.style;
  let text = k.content.text ?? '';
  if (k.content.type === 'clock') {
    const d = new Date(t * 1000);
    text = formatClock(k.content.pattern, d, k.content.language, k.content.casing);
  } else if (k.content.type === 'weather') {
    text = weatherText(k.content, 20, 3);
  } else if (k.content.type === 'sensor') {
    text = `${k.content.prefix ?? ''}${Math.round(fraction(t, e.id) * 100)}${k.content.format?.showUnit === false ? '' : '%'}${k.content.suffix ?? ''}`;
  }
  ctx.fillStyle = paint(s.paint);
  ctx.font = `${s.font?.italic ? 'italic ' : ''}${s.font?.weight ?? 400} ${s.size}px ${s.font?.family ?? 'sans-serif'}, sans-serif`;
  ctx.textAlign = s.align === 'center' ? 'center' : s.align === 'right' ? 'right' : 'left';
  ctx.textBaseline = s.valign === 'top' ? 'top' : s.valign === 'bottom' ? 'bottom' : 'middle';
  let f = e.frame;
  if (k.content.type === 'weather' && k.content.showIcon !== false) {
    const side = Math.min(s.size * 2, f.height, f.width * 0.3);
    ctx.save();
    ctx.translate(f.x, f.y + (f.height - side) / 2);
    ctx.scale(side / 24, side / 24);
    ctx.strokeStyle = paint(s.paint); ctx.lineWidth = 1.6;
    ctx.stroke(new Path2D('M5 16C-1 16 1 8 7 10C8 3 19 5 18 11C24 10 24 16 19 16Z'));
    ctx.restore();
    f = { ...f, x: f.x + side + s.size * 0.3, width: Math.max(0, f.width - side - s.size * 0.3) };
  }
  const x = s.align === 'center' ? f.x + f.width / 2 : s.align === 'right' ? f.x + f.width : f.x;
  const y = s.valign === 'top' ? f.y : s.valign === 'bottom' ? f.y + f.height : f.y + f.height / 2;
  if (k.content.type === 'weather') {
    const lines = text.split('\n'); const lineHeight = Math.ceil(s.size * 1.2);
    const top = s.valign === 'top' ? f.y : s.valign === 'bottom' ? f.y + f.height - lines.length * lineHeight : f.y + (f.height - lines.length * lineHeight) / 2;
    ctx.textBaseline = 'top';
    lines.forEach((line, i) => ctx.fillText(line, x, top + i * lineHeight, f.width));
  } else ctx.fillText(text, x, y, f.width);
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
      if (String(k.asset).toLowerCase().endsWith('.gif')) {
        ctx.fillStyle = GIF_COLORS[Math.floor((t * 1000) / DEMO_GIF_FRAME_MS) % GIF_COLORS.length];
        ctx.fillRect(f.x, f.y, f.width, f.height);
      }
      ctx.strokeStyle = 'rgba(255,255,255,0.4)';
      ctx.setLineDash([6, 6]);
      ctx.strokeRect(f.x, f.y, f.width, f.height);
      break;
    default:
      break;
  }
  ctx.restore();
}

/** A small fixed pseudo-random sequence, so the stars stay where they are. */
const STARS = Array.from({ length: 28 }, (_, i) => [((i * 7919) % 997) / 997, ((i * 104729) % 991) / 991 * 0.4, (i * 0.37) % 1]);

/**
 * One picture of the demo's video at `ms`, drawn upright in `w`×`h`: a dusk
 * sky with twinkling stars, a sun, hills that drift over a strip of grass,
 * and an orange orb crossing the sky. Every picture differs, so a playing
 * preview visibly moves; the grass is always at the bottom.
 */
function drawScene(ctx, w, h, ms) {
  const sky = ctx.createLinearGradient(0, 0, 0, h);
  sky.addColorStop(0, '#1e3a8a');
  sky.addColorStop(0.55, '#7c3aed');
  sky.addColorStop(0.85, '#f59e0b');
  ctx.fillStyle = sky;
  ctx.fillRect(0, 0, w, h);
  for (const [x, y, phase] of STARS) {
    ctx.fillStyle = `rgba(255,255,255,${0.35 + 0.65 * Math.abs(Math.sin(ms / 400 + phase * 6))})`;
    ctx.fillRect(x * w, y * h, Math.max(2, h * 0.008), Math.max(2, h * 0.008));
  }
  ctx.fillStyle = '#fde68a';
  ctx.beginPath();
  ctx.arc(w * 0.78, h * 0.62, h * 0.14, 0, Math.PI * 2);
  ctx.fill();
  const drift = (ms * 0.04) % w;
  ctx.fillStyle = '#312e81';
  ctx.beginPath();
  ctx.moveTo(0, h);
  for (let x = 0; x <= w; x += w / 48) ctx.lineTo(x, h * (0.7 + 0.06 * Math.sin(((x + drift) / w) * Math.PI * 6)));
  ctx.lineTo(w, h);
  ctx.fill();
  ctx.fillStyle = '#16a34a';
  ctx.fillRect(0, h * 0.86, w, h * 0.14);
  const loop = (ms % DEMO_VIDEO_LOOP_MS) / DEMO_VIDEO_LOOP_MS;
  const r = h * 0.11;
  ctx.fillStyle = '#f97316';
  ctx.beginPath();
  ctx.arc(-r + loop * (w + 2 * r), h * (0.32 + 0.08 * Math.sin(loop * Math.PI * 2)), r, 0, Math.PI * 2);
  ctx.fill();
}

/** How long the demo's video scene takes to loop, ms. */
export const DEMO_VIDEO_LOOP_MS = 10_200;

/**
 * The video's own picture at `ms`, as the file holds it: `source` pixels,
 * the scene upright, or turned 90° clockwise for the panel (`preTurned`,
 * the vendor's way: the scene's top on the right).
 */
function sourcePicture(video, ms) {
  const { width, height } = video.source;
  const picture = new OffscreenCanvas(width, height);
  const ctx = picture.getContext('2d');
  if (video.preTurned) {
    ctx.translate(width, 0);
    ctx.rotate(Math.PI / 2);
    drawScene(ctx, height, width, ms);
  } else {
    drawScene(ctx, width, height, ms);
  }
  return picture;
}

/**
 * What the canvas shows under the elements: the color; a video's picture
 * (`video`: playing, or its poster) turned and framed like the backend does,
 * the pad color around it with Fit; black otherwise (a picture, or a video
 * with neither poster nor decoder).
 * @param {{source: {width:number, height:number}, preTurned?: boolean, rotation: number, framing: object, box: {x:number, y:number, width:number, height:number}, ms: number}|null} video
 */
function drawBackdrop(ctx, background, canvas, video) {
  if (background?.type === 'color' || !video) {
    ctx.fillStyle = background?.type === 'color' ? color(background.color) : '#000';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    return;
  }
  ctx.fillStyle = video.framing.fit === 'contain' ? color(video.framing.padColor) : '#000';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  const { box } = video;
  const [width, height] = video.rotation % 180 === 90 ? [box.height, box.width] : [box.width, box.height];
  ctx.save();
  ctx.translate(box.x + box.width / 2, box.y + box.height / 2);
  ctx.rotate((video.rotation * Math.PI) / 180);
  ctx.drawImage(sourcePicture(video, video.ms), -width / 2, -height / 2, width, height);
  ctx.restore();
}

/**
 * Renders a theme to RGBA pixels; `video` is the video background's picture
 * the demo backend chose (see `drawBackdrop`), if any.
 * @returns {{width:number, height:number, rgba: Uint8ClampedArray}}
 */
export function renderApprox(theme, t, video = null) {
  const { width, height } = theme.canvas;
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext('2d');
  drawBackdrop(ctx, theme.background, theme.canvas, video);
  for (const e of theme.elements) if (e.visible !== false) drawElement(ctx, e, t);
  return { width, height, rgba: ctx.getImageData(0, 0, width, height).data };
}
