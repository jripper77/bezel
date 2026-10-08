import { test } from 'node:test';
import assert from 'node:assert/strict';
import { boxFromPoints, contains, handlePoints, HANDLES, hitTest, intersects, isHorizontal, isTurned, marqueeSelect, MIN_SIZE, ORIENTATIONS, orientationOf, relayoutBox, resize, roundBox, unionBox } from '../../src/editor/geometry.js';
import { snapEdge, snapMove, SNAP_DISTANCE } from '../../src/editor/snap.js';
import { boundKey, createWidget, defaultRange, textStyle, widgetOf, WIDGETS } from '../../src/editor/widgets.js';

const box = (x, y, width, height) => ({ x, y, width, height });

test('containment, intersection, marquee and union', () => {
  assert.equal(contains(box(0, 0, 10, 10), 10, 10), true);
  assert.equal(contains(box(0, 0, 10, 10), 11, 5), false);
  assert.equal(intersects(box(0, 0, 10, 10), box(10, 10, 5, 5)), true);
  assert.equal(intersects(box(0, 0, 10, 10), box(11, 0, 5, 5)), false);
  assert.deepEqual(boxFromPoints(10, 20, 0, 5), box(0, 5, 10, 15));
  assert.deepEqual(unionBox([box(0, 0, 10, 10), box(20, 5, 5, 30)]), box(0, 0, 25, 35));
  assert.equal(unionBox([]), null);
});

test('hit test finds the topmost visible element', () => {
  const els = [
    { id: 1, frame: box(0, 0, 100, 100), visible: true },
    { id: 2, frame: box(50, 50, 100, 100), visible: true },
    { id: 3, frame: box(60, 60, 10, 10), visible: false },
  ];
  assert.equal(hitTest(els, 65, 65), 2);
  assert.equal(hitTest(els, 10, 10), 1);
  assert.equal(hitTest(els, 500, 500), null);
  assert.deepEqual(marqueeSelect(els, box(55, 55, 20, 20)), [1, 2]);
});

test('resize from every handle keeps the opposite edge and a minimum size', () => {
  const b = box(100, 100, 50, 40);
  assert.deepEqual(resize(b, 'se', 10, 20), box(100, 100, 60, 60));
  assert.deepEqual(resize(b, 'nw', 10, 10), box(110, 110, 40, 30));
  assert.deepEqual(resize(b, 'w', 100, 0), box(150 - MIN_SIZE, 100, MIN_SIZE, 40));
  assert.deepEqual(resize(b, 'n', 0, 100), box(100, 140 - MIN_SIZE, 50, MIN_SIZE));
  assert.deepEqual(resize(b, 'e', -100, 0).width, MIN_SIZE);
  assert.deepEqual(resize(b, 's', 0, -100).height, MIN_SIZE);
  const ratio = resize(b, 'se', 50, 0, true);
  assert.ok(Math.abs(ratio.width / ratio.height - 1.25) < 1e-9);
  const nwRatio = resize(b, 'nw', -30, -10, true);
  assert.equal(nwRatio.x + nwRatio.width, 150);
  assert.equal(nwRatio.y + nwRatio.height, 140);
  assert.deepEqual(resize(b, 'ne', 0, 0, true), b);
  const pts = handlePoints(b);
  assert.deepEqual(Object.keys(pts), HANDLES);
  assert.deepEqual(pts.se, [150, 140]);
  assert.deepEqual(roundBox(box(0.6, 0.4, 10.2, 1)), box(1, 0, 10, MIN_SIZE));
});

test('snapping pulls onto canvas and element lines and reports guides', () => {
  const canvas = { width: 480, height: 1920 };
  const others = [box(100, 500, 200, 100)];
  const r = snapMove(box(3, 498, 50, 50), canvas, others);
  assert.deepEqual(r.box, box(0, 500, 50, 50));
  assert.deepEqual(r.guides, [{ axis: 'x', position: 0 }, { axis: 'y', position: 500 }]);
  const centered = snapMove(box(214, 900, 50, 50), canvas, []);
  assert.equal(centered.box.x, 215, 'center of the box onto the canvas center');
  const free = snapMove(box(33, 777, 50, 50), canvas, []);
  assert.deepEqual(free, { box: box(33, 777, 50, 50), guides: [] });
  assert.deepEqual(snapEdge(297, 'x', canvas, others), { value: 300, guide: 300 });
  assert.deepEqual(snapEdge(250, 'y', canvas, others), { value: 250, guide: null });
  assert.equal(SNAP_DISTANCE, 6);
});

test('every palette widget creates a valid kind sized to the canvas', () => {
  const canvas = { width: 480, height: 1920 };
  for (const w of WIDGETS) {
    const made = createWidget(w, canvas, { key: 'gpu.temperature', quantity: 'celsius' });
    assert.ok(made.width > 0 && made.height > 0, w);
    assert.ok(made.kind.type, w);
    const el = { kind: made.kind, card: made.card };
    assert.equal(widgetOf(el), w, w);
    if (['value', 'bar', 'ring', 'needle', 'graph'].includes(w)) assert.equal(boundKey(el), 'gpu.temperature', w);
  }
  assert.equal(boundKey({ kind: { type: 'image' } }), null);
  assert.throws(() => createWidget('nope', canvas), /unknown widget/);
  assert.deepEqual(defaultRange('celsius'), { min: 20, max: 100 });
  for (const q of ['megahertz', 'watts', 'rpm', 'percent']) assert.ok(defaultRange(q).max > 0);
  assert.equal(textStyle(20).align, 'left');
  assert.equal(createWidget('ring', canvas).kind.binding.key, 'cpu.usage', 'unbound defaults to cpu.usage');
});

test('orientations split into vertical/horizontal and turned or not', () => {
  assert.deepEqual(ORIENTATIONS.filter(isHorizontal), ['landscape', 'reverse-landscape']);
  assert.deepEqual(ORIENTATIONS.filter(isTurned), ['reverse-portrait', 'reverse-landscape']);
  for (const o of ORIENTATIONS) assert.equal(orientationOf(isHorizontal(o) ? 'horizontal' : 'vertical', isTurned(o)), o);
});

test('relayoutBox turns a vertical stack into a horizontal row, sizes kept, inside', () => {
  const tall = { width: 480, height: 1920 };
  const wide = { width: 1920, height: 480 };
  // Centered boxes stacked top to bottom land left to right, vertically centered.
  assert.deepEqual(relayoutBox(box(90, 300, 300, 300), tall, wide), box(300, 90, 300, 300));
  assert.deepEqual(relayoutBox(box(20, 700, 440, 200), tall, wide), box(580, 140, 440, 200));
  assert.deepEqual(relayoutBox(box(40, 80, 400, 120), tall, wide), box(0, 180, 400, 120), 'pushed right inside');
  assert.deepEqual(relayoutBox(box(0, 1800, 100, 120), tall, wide), box(1810, 0, 100, 120), 'pushed down inside');
  assert.deepEqual(relayoutBox(box(0, 0, 480, 1920), tall, wide), box(720, 0, 480, 480), 'too tall shrinks to fit');
  const back = relayoutBox(relayoutBox(box(90, 902, 300, 100), tall, wide), wide, tall);
  assert.deepEqual(back, box(90, 902, 300, 100), 'a box that fits both ways comes back');
  const square = { width: 480, height: 480 };
  assert.deepEqual(relayoutBox(box(10, 20, 30, 40), square, square), box(25, 5, 30, 40), 'a square canvas mirrors across the diagonal');
});
