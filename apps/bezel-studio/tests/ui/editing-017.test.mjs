import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createStore} from '../../src/editor/store.js';
import {DEMO_THEME} from '../../src/demo-theme.js';
const setup=()=>createStore({...DEMO_THEME,elements:[]});
const get=(s,id)=>s.getState().theme.elements.find(e=>e.id===id);
test('cut and paste a group retains position, references and complete undo',()=>{
 const s=setup();for(const widget of ['shape','text']){s.select([]);s.dispatch('add',{widget,x:100,y:100});}
 s.dispatch('groupSelection',{ids:[1,2]});const before=structuredClone(s.getState().theme);s.select([3]);s.cutSelection();assert.equal(s.getState().theme.elements.length,0);
 s.paste();const group=s.getState().theme.elements.find(e=>e.isGroup);assert.deepEqual(group.frame,before.elements.find(e=>e.id===3).frame);assert.equal(s.getState().theme.elements.filter(e=>e.groupParent===group.id).length,2);
 s.undo();s.undo();assert.deepEqual(s.getState().theme,before);
});
test('layers move selected objects into groups and card faces without moving their geometry',()=>{
 const s=setup();for(const widget of ['shape','text','shape','card']){s.select([]);s.dispatch('add',{widget,x:100,y:100});}
 s.dispatch('groupSelection',{ids:[1,2]});const group=get(s,5),original=get(s,3).frame;
 s.dispatch('placeLayer',{ids:[3],target:5,inside:true});assert.equal(get(s,3).groupParent,5);assert.deepEqual(get(s,3).frame,original);s.undo();assert.equal(get(s,3).groupParent ?? null,null);
 s.dispatch('addCardFace',{id:4,name:'Music'});s.dispatch('placeLayer',{ids:[5,3],target:4,inside:true,face:1});
 for(const id of [1,2,3,5])assert.deepEqual(get(s,id).cardMember,{parent:4,face:1});assert.equal(get(s,1).groupParent,5);
 const before=s.getState().theme;s.dispatch('placeLayer',{ids:[4],target:5,inside:true});assert.equal(s.getState().theme,before);
 s.dispatch('placeLayer',{ids:[5],root:true});assert.equal(get(s,1).cardMember,null);assert.equal(get(s,5).cardMember,null);assert.equal(get(s,1).groupParent,5);
 s.dispatch('update',{id:5,patch:{locked:true}});const locked=s.getState().theme;s.dispatch('placeLayer',{ids:[1],root:true});assert.equal(s.getState().theme,locked);
});
test('explicit trigger return targets follow face order and fall back when removed',()=>{
 const s=createStore({...DEMO_THEME,elements:[]});s.dispatch('add',{widget:'card',x:100,y:100});s.dispatch('addCardFace',{id:1,name:'Music'});s.dispatch('addCardFace',{id:1,name:'Game'});
 s.dispatch('update',{id:1,patch:{card:{triggers:[{source:'mediaPlaying',app:'Spotify',face:0,returnFace:2,returnSeconds:0,priority:0}]}}});
 s.dispatch('reorderCardFace',{id:1,direction:-1});let rule=s.getState().theme.elements[0].card.triggers[0];assert.equal(rule.returnFace,1);assert.equal(rule.face,0);
 s.dispatch('removeCardFace',{id:1});rule=s.getState().theme.elements[0].card.triggers[0];assert.equal(rule.returnFace,null);s.undo();assert.equal(s.getState().theme.elements[0].card.triggers[0].returnFace,1);
});
