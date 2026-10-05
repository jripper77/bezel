// The theme's background as the Media panel and the inspector set it: a
// video (or an animated GIF) with its poster, or a picture. Pure functions
// over the backend's answers (`list_assets`, `add_media`) and the screen.

/** Videos a background takes (the backend's list). */
export const VIDEO_EXTENSIONS = Object.freeze(['mp4', 'mov', 'm4v', 'mkv', 'webm', 'avi']);
/** Pictures the renderer draws (a GIF may also move). */
export const IMAGE_EXTENSIONS = Object.freeze(['png', 'jpg', 'jpeg', 'gif', 'svg']);

/** The lowercase extension of a file name or path (`''` without one). */
export function extensionOf(name) {
  const base = String(name ?? '').split(/[\\/]/).pop();
  const dot = base.lastIndexOf('.');
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : '';
}

/** Whether a file dropped on the window can be added: a video, a GIF or a picture. */
export function droppable(name) {
  const extension = extensionOf(name);
  return VIDEO_EXTENSIONS.includes(extension) || IMAGE_EXTENSIONS.includes(extension);
}

/** The file name of an asset reference or a path. */
export function fileNameOf(ref) {
  return String(ref ?? '').split(/[\\/]/).pop();
}

/** Whether an asset (or an `add_media` answer) moves: a video, or an animated GIF. */
export function moves(asset) {
  return asset?.kind === 'video' || Boolean(asset?.animated);
}

/**
 * The background an asset (or an `add_media` answer) makes: a video with its
 * poster when it moves, else the picture covering the canvas.
 */
export function backgroundOf(asset) {
  if (!moves(asset)) return { type: 'image', asset: asset.ref, fit: 'cover' };
  return asset.poster ? { type: 'video', asset: asset.ref, poster: asset.poster } : { type: 'video', asset: asset.ref };
}

/**
 * The Media panel's items: pictures and videos, without the posters of the
 * videos listed (each shows as its video's thumbnail).
 */
export function mediaItems(assets) {
  const posters = new Set(assets.filter((a) => moves(a) && a.poster).map((a) => a.poster));
  return assets.filter((a) => (a.kind === 'image' || a.kind === 'video') && !posters.has(a.ref));
}

/** A play time as a clock: `0:07`, `1:05`, `1:02:03`; `null` when unknown. */
export function formatDuration(ms) {
  if (typeof ms !== 'number' || !Number.isFinite(ms) || ms < 0) return null;
  const total = Math.round(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = String(total % 60).padStart(2, '0');
  return hours ? `${hours}:${String(minutes).padStart(2, '0')}:${seconds}` : `${minutes}:${seconds}`;
}

/** Play time and size of a video, as known: `['0:12', '18.9 MB']`. */
export function videoFacts(asset, bytes) {
  return [formatDuration(asset?.durationMs), typeof asset?.bytes === 'number' ? bytes(asset.bytes) : null].filter(Boolean);
}

/** Whether every model a screen may be stores and plays videos itself. */
export function playsVideos(screen) {
  const models = screen?.models ?? [];
  return models.length > 0 && models.every((m) => Boolean(m.capabilities?.storage && m.capabilities?.videoPlayback));
}

/**
 * What the inspector says about the theme's video and the connected screen:
 * - `noScreen`: no screen connected;
 * - a screen that plays stored videos: `checkWhenLive` (not live: live mode
 *   looks for the video), `checking`, `stored`, or `missing` (the storage
 *   tab sends it);
 * - a screen that cannot: `host` (live mode decodes it on this computer),
 *   `hostPlaying` (it does now) or `hostNoConverter` (no ffmpeg).
 * @param {{screen: object|null, live: boolean, liveVideo: {state: string}|null}} context
 */
export function videoStatus({ screen, live, liveVideo }) {
  if (!screen) return 'noScreen';
  const state = live ? liveVideo?.state ?? null : null;
  if (!playsVideos(screen)) {
    if (state === 'host') return 'hostPlaying';
    return state === 'noConverter' ? 'hostNoConverter' : 'host';
  }
  if (!live) return 'checkWhenLive';
  if (state === 'onDevice') return 'stored';
  return state === 'missing' ? 'missing' : 'checking';
}
