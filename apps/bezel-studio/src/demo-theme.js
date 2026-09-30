// A small theme for demo mode and tests (theme.json shape, 480x1920 portrait).

export const DEMO_THEME = Object.freeze({
  schema: 1,
  name: 'Demo',
  canvas: { width: 480, height: 1920 },
  orientation: 'reverse-portrait',
  refreshSeconds: 1,
  background: { type: 'color', color: '#0c0e16ff' },
  elements: [
    {
      id: 1,
      name: 'Clock',
      frame: { x: 40, y: 80, width: 400, height: 120 },
      opacity: 1,
      visible: true,
      locked: false,
      kind: {
        type: 'text',
        content: { type: 'clock', pattern: '%H:%M' },
        style: { font: { family: 'Inter', weight: 700, italic: false }, size: 96, paint: '#ffffffff', align: 'center', valign: 'middle', letterSpacing: 0 },
      },
    },
    {
      id: 2,
      name: 'CPU',
      frame: { x: 90, y: 300, width: 300, height: 300 },
      opacity: 1,
      visible: true,
      locked: false,
      kind: { type: 'ring', binding: { key: 'cpu.usage', min: 0, max: 100 }, startAngle: -135, sweep: 270, thickness: 24, clockwise: true, fill: '#38bdf8ff', track: '#ffffff26', roundCaps: true },
    },
    {
      id: 3,
      name: 'Background card',
      frame: { x: 20, y: 700, width: 440, height: 200 },
      opacity: 0.9,
      visible: true,
      locked: true,
      kind: { type: 'shape', shape: 'rect', radius: 20, fill: '#1e293bff', strokeWidth: 0 },
    },
  ],
});
