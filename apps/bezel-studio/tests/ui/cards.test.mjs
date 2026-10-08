import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStore } from '../../src/editor/store.js';
import { isShown } from '../../src/editor/cards.js';
import { DEMO_THEME } from '../../src/demo-theme.js';

function setup() {
  const s = createStore({ ...DEMO_THEME, elements: [] });
  s.dispatch('add', { widget: 'card', x: 240, y: 400 });
  s.dispatch('add', { widget: 'text', x: 240, y: 400 });
  s.dispatch('attachCard', { ids: [2], parent: 1, face: null });
  s.select([1]);
  s.dispatch('add', { widget: 'ring', x: 240, y: 440 });
  s.dispatch('addCardFace', { id: 1, name: 'Music' });
  s.dispatch('add', { widget: 'text', x: 240, y: 440 });
  return s;
}
const element = (s, id) => s.getState().theme.elements.find(e => e.id === id);

test('card faces share a base, hide inactive objects, and inherit visibility', () => {
  const s = setup();
  assert.deepEqual(element(s, 3).cardMember, { parent: 1, face: 0 });
  assert.deepEqual(element(s, 4).cardMember, { parent: 1, face: 1 });
  const shown = () => s.getState().theme.elements.filter(e => isShown(s.getState().theme, e)).map(e => e.id);
  assert.deepEqual(shown(), [1, 2, 4]);
  s.dispatch('cardFace', { id: 1, face: 0 });
  assert.deepEqual(shown(), [1, 2, 3]);
  s.dispatch('update', { id: 1, patch: { visible: false } });
  assert.deepEqual(shown(), []);
});

test('card moves and scales all faces, including locked children, with Undo', () => {
  const s = setup(), before = structuredClone(s.getState().theme);
  s.dispatch('update', { id: 4, patch: { locked: true } });
  s.dispatch('move', { ids: [1, 3], dx: 20, dy: 30 });
  for (const e of before.elements) assert.equal(element(s, e.id).frame.x, e.frame.x + 20);
  const f = element(s, 1).frame;
  s.dispatch('setFrame', { id: 1, frame: { ...f, width: f.width * 2, height: f.height * 2 } });
  assert.equal(element(s, 4).frame.width, before.elements[3].frame.width * 2);
  s.undo(); s.undo();
  for (const e of before.elements) assert.deepEqual(element(s, e.id).frame, e.frame);
});

test('copy/paste a card preserves every face and remaps ownership', () => {
  const s = setup(); s.select([1]); s.copySelection(); s.paste();
  assert.equal(s.getState().theme.elements.length, 8);
  assert.deepEqual(element(s, 5).card.faces, ['Face 1', 'Music']);
  for (const id of [6, 7, 8]) assert.equal(element(s, id).cardMember.parent, 5);
  s.dispatch('remove', { ids: [5] });
  assert.deepEqual(s.getState().theme.elements.map(e => e.id), [1, 2, 3, 4]);
  s.undo(); assert.equal(s.getState().theme.elements.length, 8);
});

test('copying one member detaches it rather than linking to the old card', () => {
  const s = setup(); s.select([4]); s.copySelection(); s.paste();
  assert.equal(element(s, 5).cardMember, null);
});

test('duplicate, reorder and delete faces preserve shared base and other faces', () => {
  const s = setup();
  s.dispatch('duplicateCardFace', { id: 1 });
  assert.deepEqual(element(s, 5).cardMember, { parent: 1, face: 2 });
  s.dispatch('reorderCardFace', { id: 1, direction: -1 });
  assert.deepEqual(element(s, 1).card.faces, ['Face 1', 'Music copy', 'Music']);
  assert.equal(element(s, 5).cardMember.face, 1);
  assert.equal(element(s, 4).cardMember.face, 2);
  s.dispatch('removeCardFace', { id: 1 });
  assert.equal(element(s, 5), undefined);
  assert.equal(element(s, 4).cardMember.face, 1);
  assert.equal(element(s, 2).cardMember.face, null);
  s.undo(); assert.equal(element(s, 5).cardMember.face, 1);
});

test('grouping existing objects gives one card and keeps its base below them', () => {
  const s = createStore(DEMO_THEME);
  s.dispatch('cardFromSelection', { ids: [1, 2] });
  const card = element(s, 4);
  assert.ok(card.card);
  assert.equal(element(s, 1).cardMember.parent, 4);
  assert.equal(element(s, 2).cardMember.face, 0);
  s.dispatch('reorder', { id: 4, index: 0 });
  assert.deepEqual(s.getState().theme.elements.slice(0, 3).map(e => e.id), [4, 1, 2]);
  s.dispatch('reorder', { id: 1, index: 0 });
  assert.equal(s.getState().theme.elements[0].id, 4);
});

test('membership rejects invalid face indexes and cards cannot be nested', () => {
  const s = setup();
  s.dispatch('attachCard', { ids: [4], parent: 1, face: 50 });
  assert.equal(element(s, 4).cardMember.face, 1);
  s.dispatch('attachCard', { ids: [1], parent: 1, face: 0 });
  assert.equal(element(s, 1).cardMember, undefined);
  s.dispatch('attachCard', { ids: [4], parent: null });
  assert.equal(element(s, 4).cardMember, null);
});


test('alignment and orientation changes keep card objects together', () => {
  const s = setup();
  const offset = () => ({ x: element(s, 4).frame.x - element(s, 1).frame.x, y: element(s, 4).frame.y - element(s, 1).frame.y });
  const before = offset();
  s.dispatch('align', { ids: [1], edge: 'left' });
  assert.equal(element(s, 1).frame.x, 0);
  assert.deepEqual(offset(), before);
  s.dispatch('setOrientation', { orientation: 'landscape' });
  assert.deepEqual(offset(), before);
});

test('reordering and removing faces remaps rules with Undo',()=>{
 const s=setup();s.dispatch('update',{id:1,patch:{card:{triggers:[{source:'mediaPlaying',app:'Spotify',face:1,priority:0,returnSeconds:3},{source:'process',app:'game.exe',face:0,priority:1,returnSeconds:0}]}}});
 s.dispatch('reorderCardFace',{id:1,direction:-1});assert.deepEqual(element(s,1).card.triggers.map(r=>r.face),[0,1]);
 s.dispatch('removeCardFace',{id:1});assert.deepEqual(element(s,1).card.triggers.map(r=>r.face),[0]);
 s.undo();assert.deepEqual(element(s,1).card.triggers.map(r=>r.face),[0,1]);
});
