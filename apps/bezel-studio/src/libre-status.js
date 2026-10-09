/** Reading health, grouped by hardware so one healthy CPU cannot hide a failed PSU.
 * Optional sensors may be missing; a hardware group needs one physical reading.
 * Corsair's composite Total Output can report zero when every rail is missing.
 * `hardware` lists every group, in catalog order, with whether it reads; `failed`
 * names the ones that do not.
 */
export function libreStatus(catalog, readings) {
  const sensors = catalog.filter((s) => s.source.startsWith('LibreHardwareMonitor'));
  if (!sensors.length) return null;
  const groups = new Map();
  for (const sensor of sensors) {
    const group = sensor.key.startsWith('lhm.')
      ? sensor.key.split('.').slice(0, -2).join('.')
      : sensor.category;
    if (!groups.has(group)) groups.set(group, []);
    // The PSU's physical voltage/current/temperature readings establish health,
    // rather than a synthetic power total that can disguise missing USB replies.
    if (sensor.key.startsWith('lhm.psu.corsair.') && sensor.quantity === 'watts') continue;
    groups.get(group).push(sensor);
  }
  const hardware = [...groups].map(([group, entries]) => ({
    name: entries[0]?.label ?? group,
    ok: entries.some((s) => {
      const reading = readings?.[s.key];
      return !reading?.unavailable && Number.isFinite(reading?.value);
    }),
  }));
  const failed = hardware.filter((h) => !h.ok).map((h) => h.name);
  return { state: !failed.length ? 'ok' : failed.length === groups.size ? 'error' : 'partial', failed, hardware };
}
