// The app icons: the Pixel mark SVGs and the raster set Tauri ships.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { PALETTE, matrixPaths, pixelMarkSvg } from '../../scripts/build-icons.mjs';

const app = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const icons = join(app, 'src-tauri', 'icons');
const conf = JSON.parse(readFileSync(join(app, 'src-tauri', 'tauri.conf.json'), 'utf8'));

/** Width and height from the IHDR chunk of a PNG. */
function pngSize(buf) {
  assert.equal(buf.subarray(0, 8).toString('hex'), '89504e470d0a1a0a', 'PNG signature');
  assert.equal(buf.subarray(12, 16).toString('latin1'), 'IHDR');
  return [buf.readUInt32BE(16), buf.readUInt32BE(20)];
}

/** Square sizes stored in an ICO directory (a 0 byte means 256). */
function icoSizes(buf) {
  assert.equal(buf.readUInt16LE(0), 0, 'ICO reserved');
  assert.equal(buf.readUInt16LE(2), 1, 'ICO type = icon');
  const sizes = [];
  for (let i = 0; i < buf.readUInt16LE(4); i += 1) {
    const at = 6 + i * 16;
    const w = buf[at] || 256;
    const h = buf[at + 1] || 256;
    assert.equal(w, h, `ICO entry ${i} is square`);
    sizes.push(w);
  }
  return sizes.sort((a, b) => a - b);
}

/** The edge the file name promises, or undefined for an unknown PNG. */
function expectedEdge(name) {
  const fixed = { 'icon.png': 512, 'icon-1024.png': 1024, 'tray.png': 64, 'StoreLogo.png': 50 };
  if (name in fixed) return fixed[name];
  const m = /^(?:Square)?(\d+)x(\d+)(@2x)?(?:Logo)?\.png$/.exec(name);
  if (!m) return undefined;
  assert.equal(m[1], m[2], `${name} names a square`);
  return Number(m[1]) * (m[3] ? 2 : 1);
}

test('both icon.svg files are the generated Pixel mark', () => {
  const ui = readFileSync(join(app, 'src', 'icon.svg'), 'utf8');
  const tauri = readFileSync(join(icons, 'icon.svg'), 'utf8');
  assert.equal(ui, tauri, 'src/icon.svg and src-tauri/icons/icon.svg differ');
  assert.equal(ui, pixelMarkSvg(), 'icon.svg is stale: run node scripts/build-icons.mjs');
  assert.match(ui, /fill="#FF9248"/, 'the accent is the literal --accent');
});

test('the mark is a B on a 5x7 matrix with an accent base and a live pixel', () => {
  const cells = (d) => (d.match(/M/g) ?? []).length;
  const p = matrixPaths();
  assert.equal(cells(p.dim) + cells(p.lit) + cells(p.acc) + cells(p.live), 35);
  assert.equal(cells(p.live), 1);
  assert.equal(cells(p.acc), 6);
  assert.equal(cells(p.lit), 14);
  assert.equal(PALETTE.accent, '#FF9248');
});

test('every PNG listed in tauri.conf.json has the size in its name', () => {
  const listed = conf.bundle.icon.filter((f) => f.endsWith('.png'));
  assert.ok(listed.length >= 3);
  for (const rel of listed) {
    const name = rel.split('/').pop();
    const edge = expectedEdge(name);
    assert.deepEqual(pngSize(readFileSync(join(app, 'src-tauri', rel))), [edge, edge], rel);
  }
});

test('every PNG in src-tauri/icons, extras included, has the size in its name', () => {
  const pngs = readdirSync(icons).filter((f) => f.endsWith('.png'));
  for (const extra of ['icon-1024.png', 'tray.png', '64x64.png']) assert.ok(pngs.includes(extra), extra);
  for (const name of pngs) {
    const edge = expectedEdge(name);
    assert.ok(edge, `unexpected icon ${name}`);
    assert.deepEqual(pngSize(readFileSync(join(icons, name))), [edge, edge], name);
  }
});

test('icon.ico (NSIS installer and exe icon) carries 16/24/32/48/64/256', () => {
  assert.ok(conf.bundle.icon.includes('icons/icon.ico'));
  assert.deepEqual(icoSizes(readFileSync(join(icons, 'icon.ico'))), [16, 24, 32, 48, 64, 256]);
});

test('icon.icns is an Apple icon family', () => {
  const buf = readFileSync(join(icons, 'icon.icns'));
  assert.equal(buf.subarray(0, 4).toString('latin1'), 'icns');
  assert.equal(buf.readUInt32BE(4), buf.length);
});
