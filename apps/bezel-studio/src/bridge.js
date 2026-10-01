// The only module that knows where data comes from: Tauri's IPC in the app,
// or an in-memory demo backend in a browser on localhost (Playwright, UI work
// without hardware). Both expose the same async API.
import { createDemoBackend } from './demo-backend.js';

/** The renderer's "nothing animates" in a frame header. */
export const STILL = 0xffffffff;

/**
 * Parses the renderer's frame: u32 LE width, u32 LE height, u32 LE
 * milliseconds until its next picture is due — its animated GIFs change or
 * its video background shows the next picture (`STILL`: nothing moves) —
 * then RGBA8. `nextMs` is `null` for a still frame.
 */
export function parseFrame(buffer) {
  const bytes = buffer instanceof ArrayBuffer ? new Uint8Array(buffer) : new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  if (bytes.length < 12) throw new Error('frame too short');
  const view = new DataView(bytes.buffer, bytes.byteOffset, 12);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  const next = view.getUint32(8, true);
  const rgba = new Uint8ClampedArray(bytes.buffer, bytes.byteOffset + 12, bytes.length - 12);
  if (rgba.length !== width * height * 4) throw new Error(`frame is ${rgba.length} bytes, expected ${width * height * 4}`);
  return { width, height, rgba, nextMs: next === STILL ? null : next };
}

/**
 * Event of a running storage job's progress (`storage-progress` in the
 * backend): an upload, or a storage manager job (a plan run or a batch
 * delete), whose reports also name the file they are at.
 * @typedef {{
 *   phase: 'convert'|'upload'|'verify'|'delete',
 *   done: number,
 *   total: number,
 *   step?: {index: number, count: number, source: string, target: string|null},
 * }} ProgressDto `step` (manager jobs only): the file's place in the job
 *   (0-based `index` of `count`), its screen path and where its copy goes
 *   (`null` for a delete). `upload` counts bytes; `verify` and a plan's
 *   `delete` go 0 → 1; a batch delete's `delete` counts files.
 */
export const PROGRESS_EVENT = 'storage-progress';

// ------------------------------------------------- storage manager DTOs --
// The storage manager (D-2026-09-30-storage-manager-4, -13). Every call
// takes the screen's key like the storage tab's; the backend keys the catalog
// by the screen's model (and the user's name for it). Paths are
// `<internal|sd>/<image|video>/<name>`. Errors reject like every command,
// as `{code, args, message}` (`busy`, `notConfirmed`, `stale`, `unsupported`,
// `invalidInput`, `unknownMedium`, ...); refusals of a plan are answers.
/**
 * @typedef {'internal'|'sd'} MediumCode
 * @typedef {{total: number, used: number, free: number}} CapacityDto
 * @typedef {{path: string, medium: MediumCode, kind: 'image'|'video', name: string, size: number|null}} StoredFileDto
 *   `size` is `null` when the screen cannot tell it and the catalog has none (TUR_USB).
 * @typedef {{
 *   state: 'pending'|'stored'|'missing'|'deleted',
 *   localCopy: boolean,
 *   sentAt: number,
 *   source: string|null,
 *   durationMs: number|null,
 *   resolution: {width: number, height: number}|null,
 * }} CatalogEntryDto A file Bezel sent (or associated): `localCopy` when the
 *   store holds its exact bytes; `sentAt` in seconds since the epoch.
 * @typedef {{
 *   code: 'duplicate'|'hangPartial'|'pending'|'variant'|'sizeDiffers'|'sameSize'|'unused',
 *   prechecked: boolean,
 *   kept: string|null,
 *   cataloged: number|null,
 * }} FindingDto A cleanup finding (core `cleanup::Finding`): `kept` is the
 *   path of the file that stays (`duplicate`, `variant`, `sameSize`),
 *   `cataloged` the size Bezel stored (`sizeDiffers`).
 * @typedef {StoredFileDto & {
 *   entry: CatalogEntryDto|null,
 *   finding: FindingDto|null,
 *   protected: 'boot'|'themeVideo'|null,
 * }} ManagedFileDto A listed file with its catalog entry at that place (none
 *   when Bezel did not send it), its cleanup finding, and whether it is the
 *   boot media Bezel set or a video a theme plays.
 * @typedef {StoredFileDto & {
 *   id: string, size: number, sentAt: number, localCopy: boolean, otherCard: boolean,
 *   state: 'pending'|'stored'|'missing'|'deleted',
 * }} RestorableDto A cataloged file the screen does not store now: missing
 *   from its medium, on another card (`otherCard`), or deleted through Bezel
 *   with its local copy held (`deleted`: offered, never chosen by default).
 *   `id` names it to `planRestore`.
 * @typedef {{copies: number, bytes: number, deletedCopies: number, deletedBytes: number, limit: number}} CacheDto
 *   The local copies; those of files deleted through Bezel count against `limit`.
 * @typedef {{
 *   internal: CapacityDto,
 *   card: CapacityDto|null,
 *   files: ManagedFileDto[],
 *   folderErrors: {medium: MediumCode, kind: 'image'|'video', error: {code: string, args: object, message: string}}[],
 *   restorable: RestorableDto[],
 *   deletes: boolean,
 *   cap: number,
 *   cache: CacheDto,
 * }} ManagerOverviewDto Both media, listed and reconciled with the catalog;
 *   `deletes` is false on screens that cannot delete through Bezel (TUR_USB);
 *   `cap` is the screen's per-file limit in bytes.
 * @typedef {{source: string, target: string, size: number, replaces: StoredFileDto|null}} PlanStepDto
 *   One file of a plan: its copy goes to `target`, is checked, and then (move,
 *   rename) `source` is deleted. `replaces`: the file there whose overwrite was asked.
 * @typedef {{source: string, target: string, code: 'conflict'|'noLocalCopy'|'deleteUnsupported'|'present', conflict: StoredFileDto|null}} SkippedDto
 *   A file the plan leaves out; `conflict` is the file of that name already
 *   there (or sent by another step). A conflict is planned again with its
 *   `target` in `overwrite` once the user confirms the overwrite.
 * @typedef {{code: 'bootMedia'|'themeVideo', path: string}} PlanWarningDto
 * @typedef {{
 *   status: 'ready', ticket: number, transfer: 'move'|'copy'|'rename'|'restore', to: MediumCode,
 *   steps: PlanStepDto[], skipped: SkippedDto[], warnings: PlanWarningDto[], bytes: number, free: number,
 * }} PlanReadyDto What the one confirmation lists; `free` is the target medium's free space.
 * @typedef {{
 *   status: 'refused',
 *   code: 'noCard'|'notListed'|'sameMedium'|'invalidName'|'extensionChanged'|'sameName'|'unsendable'|'noSpace',
 *   args: object,
 *   message: string,
 * }} PlanRefusedDto Nothing was sent. `args`: `notListed`/`sameMedium` `{path}`;
 *   `invalidName` `{char?}`; `extensionChanged` `{expected: string|null}`;
 *   `unsendable` `{path, refusal}` (a preflight refusal, as `prepareUpload`'s);
 *   `noSpace` `{needed, free}`.
 * @typedef {{
 *   halt: 'failed'|'refused'|'sourceChanged'|'noLocalCopy'|'conflict'|'cancelled',
 *   error: {code: string, args: object, message: string}|null,
 *   refusal: object|null,
 *   conflict: StoredFileDto|null,
 * }} HaltDto Why a file stopped its batch (core `manager::Halt` code): `error`
 *   for `failed`, the target's preflight `refusal` for `refused`, the file
 *   of the target's name for `conflict`; `sourceChanged` (gone or changed
 *   since the list) and `noLocalCopy` carry nothing more.
 * @typedef {'preflight'|'upload'|'verify'|'delete'|'catalog'} StageCode How
 *   far a stopped file had come (core `manager::Stage`): from `delete` on its
 *   copy is verified at the target; at `catalog` its source is deleted too.
 * @typedef {{
 *   transfer: 'move'|'copy'|'rename'|'restore',
 *   done: PlanStepDto[],
 *   failed: ({step: PlanStepDto, stage: StageCode} & HaltDto)|null,
 *   cancelled: {step: PlanStepDto, stage: StageCode, partial: number|null}|null,
 *   notStarted: PlanStepDto[],
 * }} TransferReportDto A run stops at the first failure (a backend error, the
 *   preflight's refusal on the target, the source changed, the copy gone, the
 *   name taken) or on Cancel; `stage` says what became of that file's source.
 *   `partial`: bytes a cancelled upload left at the target.
 * @typedef {({status: 'ran'} & TransferReportDto)|PlanRefusedDto} RunDto What
 *   `runPlan` answers: the report, or a restore refused before anything was
 *   sent because it no longer fits (`noSpace`, `unsendable`, `noCard`).
 * @typedef {{deleted: string[], failed: ({path: string} & HaltDto)|null, cancelled: boolean, notStarted: string[], freed: number}} DeleteReportDto
 *   `failed.halt` is `sourceChanged` for a file gone or of another size than
 *   confirmed, `failed` for a screen error.
 * @typedef {{source: string, name: string, size: number, kind: 'image'|'video', durationMs: number|null, resolution: {width: number, height: number}|null, sameName: boolean}} CandidateDto
 *   A file on the PC of exactly the screen file's size and kind.
 */

// ------------------------------------------- video background framing --
// The video background plays in the preview and is framed
// (D-2026-10-01-video-background-framing-2, -3, -5, -6). The theme JSON the
// UI sends to `render_preview`, `push_theme` and `save_theme` carries the
// framing inside the video background; `video_auto` says what Auto is.
/**
 * @typedef {{
 *   rotation?: 0|90|180|270,
 *   fit?: 'cover'|'contain',
 *   zoom?: number,
 *   position?: {x: number, y: number},
 *   padColor?: string,
 * }} FramingDto The optional `framing` of a video background
 *   (`theme.background = {type: 'video', asset, poster?, framing?}`), exactly
 *   the `.bezeltheme` JSON (schema 1):
 *   - `rotation`: clockwise degrees the video is turned; absent = Auto;
 *   - `fit`: `cover` (Fill, the default: no blank edge) or `contain` (Fit:
 *     the whole picture, `padColor` around it);
 *   - `zoom`: 1.0–4.0 on top of the fit's scale (default 1.0);
 *   - `position`: `x` and `y` in 0–1 with CSS `object-position` semantics
 *     (default 0.5 each): on an axis where the picture overflows it picks
 *     the part shown, where it is smaller it places the picture;
 *   - `padColor`: `#rrggbbaa`, opaque (default `#000000ff`).
 *   The UI leaves out every key at its default, and `framing` itself when
 *   all are (adding or replacing a video writes none); it rounds `zoom` to
 *   hundredths and `position` to thousandths. The backend takes a missing
 *   key as its default, clamps numbers out of range and refuses another
 *   rotation with `invalidInput`. Example (the user's Fit at 125 %):
 *   `{"rotation": 270, "fit": "contain", "zoom": 1.25, "position": {"x": 0.5, "y": 0.4}, "padColor": "#000000ff"}`.
 * @typedef {{rotation: 0|90|180|270, size: {width: number, height: number}|null}} VideoAutoDto
 *   What Auto resolves to for the theme's video background (`video_auto`):
 *   - `size`: the video's own size as the backend probes it without ffmpeg
 *     (`bezel-media`'s probe: the MP4/MOV boxes, a GIF's header; the
 *     `MediaInfo.dimensions`); `null` when the probe gives none (a missing
 *     or broken file, another container);
 *   - `rotation`: the clockwise turn Auto gives the video. When `size` is
 *     exactly the panel's native size (480x1920 on the 8.8") and the theme
 *     is an odd number of quarter turns from the panel, the video is taken
 *     as already turned for the panel (the vendor keeps every theme video
 *     panel-native) and gets the turns that cancel the theme-to-panel turns
 *     (a landscape theme on the 8.8", the Dragon Ball case: 270, so 0 turns
 *     in total); any other size, an unknown size or a half-turned theme: 0.
 *   The panel is the live screen's; with no screen live, the catalog's
 *   panel whose native size is the canvas turned back (one size, e.g. the
 *   8.8"'s for a 1920x480 canvas); without one, `rotation` is 0. The
 *   preview's Auto is resolved the same way. A theme without a video
 *   background answers `{rotation: 0, size: null}`.
 * @typedef {{width: number, height: number, rgba: Uint8ClampedArray, nextMs: number|null, millis: number}} PreviewFrameDto
 *   A preview frame (`parseFrame`) and how long the render took, ms.
 */

/**
 * Event the app sends when the window's close button is pressed with unsaved
 * edits and no screen live: the UI asks, then calls `closeWindow`.
 */
export const CLOSE_EVENT = 'close-requested';

/** Demo mode: the window event that stands for the close button. */
export const DEMO_CLOSE_EVENT = 'bezel-demo-close';

/**
 * Event the app sends when the tray's Quit is chosen with unsaved edits:
 * the UI asks, then calls `quitApp`.
 */
export const QUIT_EVENT = 'quit-requested';

/** Demo mode: the window event that stands for the tray's Quit. */
export const DEMO_QUIT_EVENT = 'bezel-demo-quit';

/**
 * Demo mode with `?hold` (tests only): every phase of an upload waits in
 * the middle until the page sends this window event, once per phase.
 */
export const DEMO_LET_GO_EVENT = 'bezel-demo-let-go';

/**
 * Subscribes to files dropped on the window from the system: Tauri owns the
 * drag and reports the paths and the pointer (physical pixels).
 */
function onFileDrop(tauri, cb) {
  const webview = tauri.webview?.getCurrentWebview?.();
  if (webview?.onDragDropEvent) return webview.onDragDropEvent((e) => cb(e.payload));
  const listen = tauri.event?.listen;
  if (typeof listen !== 'function') return Promise.resolve(() => {});
  const events = { 'tauri://drag-over': 'over', 'tauri://drag-drop': 'drop', 'tauri://drag-leave': 'leave' };
  return Promise.all(Object.entries(events).map(([name, type]) => listen(name, (e) => cb({ type, ...e.payload }))));
}

function tauriBridge(invoke, tauri = {}) {
  return {
    mode: 'tauri',
    listDevices: () => invoke('list_devices'),
    leaveDesktopMode: (key, confirmed) => invoke('leave_desktop_mode', { key, confirmed }),
    catalog: () => invoke('sensor_catalog'),
    sample: () => invoke('sample_sensors'),
    session: () => invoke('editor_session'),
    /**
     * Renders the theme for the preview: `render_preview {theme, motion}`.
     * With `motion` (the default) a video background plays: the backend
     * keeps at most one preview decoder per session (ffmpeg decodes the raw
     * source, at most 15 pictures a second, and Rust turns and frames each
     * picture with the theme's framing), started by the first frame asked
     * for and ended when none is asked for during 2 s (a hidden window); it
     * resumes from the clock at the next frame. Changing the framing never
     * restarts it; another video does. `motion: false` (motion reduced, or
     * the window hidden) shows the video's poster and starts no decoder.
     * Without ffmpeg the poster shows too (the UI says why). The frame's
     * `nextMs` is when its next picture is due (the video's next picture or
     * a GIF's, whichever is first; at least 1000/15 ms apart for the
     * video), `null` when nothing moves (no motion, no ffmpeg, a still theme).
     * @param {object} theme
     * @param {{motion?: boolean}} [options]
     * @returns {Promise<PreviewFrameDto>}
     */
    render: async (theme, { motion = true } = {}) => {
      const started = performance.now();
      const frame = parseFrame(await invoke('render_preview', { theme, motion }));
      return { ...frame, millis: performance.now() - started };
    },
    /**
     * What Auto resolves to for the theme's video background: `video_auto
     * {theme}` (the theme as edited, unsaved edits included). Rejects only
     * like every command (`{code, args, message}`, e.g. `invalidInput` for
     * a theme the backend cannot read).
     * @returns {Promise<VideoAutoDto>}
     */
    videoAuto: (theme) => invoke('video_auto', { theme }),
    /**
     * Opens a page of the user guide in the system's browser: `open_guide
     * {page, language}`. `page`: `ffmpeg` (installing ffmpeg,
     * `docs/user/ffmpeg.md`); `language`: `pt-BR` (the guide's
     * `docs/user/pt-BR/` page) or `en`. The backend maps the page to its
     * fixed URL on the project's site
     * (`https://github.com/slipalison/bezel/blob/main/docs/user/[pt-BR/]ffmpeg.md`),
     * so the webview never navigates and no other URL can be opened; an
     * unknown page or language rejects with `invalidInput`.
     * @param {'ffmpeg'} page
     * @param {'pt-BR'|'en'} language
     * @returns {Promise<void>}
     */
    openGuide: (page, language) => invoke('open_guide', { page, language }),
    pushTheme: (theme) => invoke('push_theme', { theme }),
    setLive: (on, screen) => invoke('set_live', { on, screen }),
    setBrightness: (screen, percent) => invoke('set_brightness', { screen, percent }),
    release: (screen) => invoke('release_screen', { screen }),
    restartScreen: (screen) => invoke('restart_screen', { screen }),
    saveTheme: (theme, saveAs) => invoke('save_theme', { theme, saveAs }),
    listThemes: () => invoke('list_themes'),
    openTheme: (location) => invoke('open_theme', { location }),
    newTheme: (screen, name, orientation) => invoke('new_theme', { screen, name, orientation }),
    importTheme: () => invoke('import_theme'),
    addImage: () => invoke('add_image'),
    // A dropped file's path, or `null` to ask with the native dialog.
    addMedia: (path = null) => invoke('add_media', { path }),
    assets: () => invoke('list_assets'),
    fonts: () => invoke('list_fonts'),
    getAutostart: () => invoke('get_autostart'),
    setAutostart: (on) => invoke('set_autostart', { on }),
    storageOverview: (screen) => invoke('storage_overview', { screen }),
    mediaTools: () => invoke('media_tools'),
    locateFfmpeg: () => invoke('locate_ffmpeg'),
    pickMedia: () => invoke('pick_media'),
    prepareUpload: (screen, source, medium) => invoke('prepare_upload', { screen, source, medium }),
    prepareThemeVideo: (screen) => invoke('prepare_theme_video', { screen }),
    runUpload: (ticket, overwrite) => invoke('run_upload', { ticket, overwrite }),
    cancelJob: () => invoke('cancel_job'),
    deleteStored: (screen, path, confirmed) => invoke('delete_stored', { screen, path, confirmed }),
    playStored: (screen, path) => invoke('play_stored', { screen, path }),
    stopPlayback: (screen) => invoke('stop_playback', { screen }),
    setBootMedia: (screen, path, confirmed, brightness = null) => invoke('set_boot_media', { screen, path, confirmed, brightness }),
    // ------------------------------------------- the storage manager --
    /** @returns {Promise<ManagerOverviewDto>} */
    managerOverview: (screen) => invoke('manager_overview', { screen }),
    /** A file's thumbnail from its local copy, as a `data:` URL, or `null`. @returns {Promise<string|null>} */
    managerThumbnail: (screen, path) => invoke('manager_thumbnail', { screen, path }),
    /** @returns {Promise<PlanReadyDto|PlanRefusedDto>} */
    planMove: (screen, paths, to, overwrite = []) => invoke('plan_move', { screen, paths, to, overwrite }),
    /** @returns {Promise<PlanReadyDto|PlanRefusedDto>} */
    planCopy: (screen, paths, to, overwrite = []) => invoke('plan_copy', { screen, paths, to, overwrite }),
    /** @returns {Promise<PlanReadyDto|PlanRefusedDto>} */
    planRename: (screen, path, newName, overwrite = []) => invoke('plan_rename', { screen, path, newName, overwrite }),
    /** `ids`: `RestorableDto.id`s. @returns {Promise<PlanReadyDto|PlanRefusedDto>} */
    planRestore: (screen, ids, to, overwrite = []) => invoke('plan_restore', { screen, ids, to, overwrite }),
    /**
     * Runs a plan; `confirmed` is the answer to the dialog that listed every file (without it nothing runs).
     * Progress comes as `storage-progress`, Cancel is `cancelJob`. @returns {Promise<RunDto>}
     */
    runPlan: (ticket, confirmed) => invoke('run_plan', { ticket, confirmed }),
    /**
     * Deletes the confirmed files one by one (a cleanup or a selection): `files` as the dialog listed them,
     * `{path, size}`; one gone or of another size by then is not deleted. @returns {Promise<DeleteReportDto>}
     */
    deleteFiles: (screen, files, confirmed) => invoke('delete_files', { screen, files, confirmed }),
    /** Originals chosen on the PC: files, or one folder (`folder`); `[]` when cancelled. @returns {Promise<string[]>} */
    pickOriginals: (folder) => invoke('pick_originals', { folder }),
    /** The originals among `sources` (files or folders), likeliest first. @returns {Promise<{candidates: CandidateDto[]}>} */
    associateCandidates: (screen, path, sources) => invoke('associate_candidates', { screen, path, sources }),
    /** Copies a confirmed original into the store. @returns {Promise<ManagedFileDto>} */
    associateOriginal: (screen, path, source, confirmed) => invoke('associate_original', { screen, path, source, confirmed }),
    /** @returns {Promise<CacheDto>} */
    cacheInfo: () => invoke('cache_info'),
    /** `scope`: `deleted` (copies of deleted files) or `all`. @returns {Promise<{removed: number, bytes: number}>} */
    clearCache: (scope, confirmed) => invoke('clear_cache', { scope, confirmed }),
    /** @returns {Promise<CacheDto>} */
    setCacheLimit: (bytes) => invoke('set_cache_limit', { bytes }),
    setUnsaved: (unsaved) => invoke('set_unsaved', { unsaved }),
    closeWindow: () => invoke('close_window'),
    quitApp: () => invoke('quit_app'),
    preferences: () => invoke('preferences'),
    setLanguage: (language) => invoke('set_language', { language }),
    setSensorOptions: (pingHost, mangohudDir) => invoke('set_sensor_options', { pingHost, mangohudDir }),
    pickFolder: () => invoke('pick_folder'),
    showSensors: (keys) => invoke('show_sensors', { keys }),
    // A library theme's thumbnail (a `data:` URL), or `null` when it cannot be drawn.
    themeThumbnail: (location) => invoke('theme_thumbnail', { location }),
    setThemeFilter: (scope, axis) => invoke('set_theme_filter', { scope, axis }),
    onJobProgress: (cb) => (typeof tauri.event?.listen === 'function' ? tauri.event.listen(PROGRESS_EVENT, (e) => cb(e.payload)) : Promise.resolve(() => {})),
    onCloseRequested: (cb) => (typeof tauri.event?.listen === 'function' ? tauri.event.listen(CLOSE_EVENT, () => cb()) : Promise.resolve(() => {})),
    onQuitRequested: (cb) => (typeof tauri.event?.listen === 'function' ? tauri.event.listen(QUIT_EVENT, () => cb()) : Promise.resolve(() => {})),
    onFileDrop: (cb) => onFileDrop(tauri, cb),
    // Files dropped in the webview carry no path: the system drop above does.
    fileSource: () => null,
  };
}

/**
 * Picks the backend for this page.
 * @param {{__TAURI__?: any, location: {hostname: string, search: string}}} win
 */
export function createBridge(win) {
  const invoke = win.__TAURI__?.core?.invoke;
  if (typeof invoke === 'function') return tauriBridge(invoke, win.__TAURI__);
  const local = ['localhost', '127.0.0.1'].includes(win.location.hostname);
  if (!local) {
    const fail = () => Promise.reject(new Error('no backend'));
    return new Proxy({ mode: 'unavailable' }, { get: (t, k) => (k in t ? t[k] : fail) });
  }
  const params = new URLSearchParams(win.location.search);
  const scenario = params.get('demo') ?? 'turing88';
  // What the window does is shown on the page (`data-demo-window`), and the
  // close button is a window event: Playwright drives and checks both. So
  // are the sensors the list shows (`data-demo-sensors`).
  const root = win.document?.documentElement;
  // So are the preview's video decoder and the guide pages opened.
  const demo = createDemoBackend(scenario, {}, {
    onWindow: (state) => root?.setAttribute('data-demo-window', state),
    onSensorsShown: (keys) => root?.setAttribute('data-demo-sensors', keys.join(' ')),
    onDecoder: (state) => root?.setAttribute('data-demo-decoder', state),
    onGuide: (page, language) => root?.setAttribute('data-demo-guide', `${page} ${language}`),
    languages: win.navigator?.languages ?? [],
    hold: params.has('hold'),
  });
  win.addEventListener?.(DEMO_CLOSE_EVENT, () => demo.requestClose());
  win.addEventListener?.(DEMO_QUIT_EVENT, () => demo.requestQuit());
  win.addEventListener?.(DEMO_LET_GO_EVENT, () => demo.letGo());
  return { mode: 'demo', ...demo };
}
