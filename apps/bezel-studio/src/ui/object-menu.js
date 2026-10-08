import {el} from './dom.js';
import {canGroup,groupTarget,isShown,owners} from '../editor/cards.js';
import {hitTest} from '../editor/geometry.js';

export function wireObjectMenu({store,canvasView,stage,layers,t}) {
 let menu=null;
 const focusStage=()=>{stage.tabIndex=0;stage.focus({preventScroll:true});};
 function close(focus=false){menu?.remove();menu=null;if(focus)focusStage();}
 function open(evt,keyboard=false){
  if(canvasView.framing() || document.querySelector('dialog[open]'))return;
  const row=evt.target.closest('.layer-row');
  if(!row && !stage.contains(evt.target) && !layers.contains(evt.target))return;
  let {theme,selection}=store.getState();let id=row?Number(row.dataset.elementId):null;
  if(!row && !keyboard){const p=canvasView.point(evt.clientX,evt.clientY);id=hitTest(theme.elements.filter(e=>isShown(theme,e)),p.x,p.y);if(id!==null&&!selection.includes(id))id=groupTarget(theme,id);}
  if(id!==null&&!selection.includes(id)){store.select([id]);selection=[id];}
  if(!selection.length&&!store.canPaste())return;
  evt.preventDefault();close();
  const selected=theme.elements.filter(e=>selection.includes(e.id));
  const editable=selected.some(e=>!owners(theme,e).some(p=>p.locked));
  const action=(label,run,disabled=false)=>el('button',{type:'button',role:'menuitem',disabled,text:label,onclick:()=>{close();run();focusStage();}});
  const dispatch=(command,args={})=>()=>store.dispatch(command,{ids:selection,...args});
  menu=el('div',{class:'object-menu',role:'menu','aria-label':t('menu.title')},[
   action(t('menu.cut'),()=>store.cutSelection(),!editable),action(t('menu.copy'),()=>store.copySelection(),!selected.length),
   action(t('menu.paste'),()=>store.paste(),!store.canPaste()),action(t('inspector.duplicate'),dispatch('duplicate'),!selected.length),
   el('hr',{role:'separator'}),action(t('group.create'),dispatch('groupSelection'),!canGroup(theme,selection)),
   action(t('group.ungroup'),dispatch('ungroup'),!selected.some(e=>e.isGroup&&!e.locked)),
   action(t('card.group'),dispatch('cardFromSelection'),!editable||selected.some(e=>e.card)),
   el('hr',{role:'separator'}),action(t('inspector.front'),dispatch('orderSelection',{front:true}),!selected.length),
   action(t('inspector.back'),dispatch('orderSelection',{front:false}),!selected.length),
   action(t('menu.detach'),dispatch('placeLayer',{root:true}),!selected.some(e=>e.groupParent!=null||e.cardMember)),
   action(t(selected.every(e=>e.locked)?'layers.unlock':'layers.lock'),dispatch('updateSelection',{patch:{locked:!selected.every(e=>e.locked)}}),!selected.length),
   action(t(selected.every(e=>e.visible===false)?'layers.show':'layers.hide'),dispatch('updateSelection',{patch:{visible:selected.every(e=>e.visible===false)}}),!selected.length),
   el('hr',{role:'separator'}),action(t('inspector.delete'),dispatch('remove',{ids:selected.filter(e=>!owners(theme,e).some(p=>p.locked)).map(e=>e.id)}),!editable),
  ]);
  document.body.append(menu);
  const box=keyboard?evt.target.getBoundingClientRect():null;
  const x=keyboard?box.left:evt.clientX,y=keyboard?box.bottom:evt.clientY;
  menu.style.left=`${Math.max(4,Math.min(x,innerWidth-menu.offsetWidth-4))}px`;
  menu.style.top=`${Math.max(4,Math.min(y,innerHeight-menu.offsetHeight-4))}px`;
  menu.querySelector('button:not(:disabled)')?.focus();
  menu.addEventListener('keydown',evt=>{
   const buttons=[...menu.querySelectorAll('button:not(:disabled)')];const index=buttons.indexOf(document.activeElement);
   if(evt.key==='Escape'){evt.preventDefault();evt.stopPropagation();close(true);}
   else if(['ArrowDown','ArrowUp','Home','End'].includes(evt.key)){evt.preventDefault();evt.stopPropagation();buttons[evt.key==='Home'?0:evt.key==='End'?buttons.length-1:(index+(evt.key==='ArrowDown'?1:-1)+buttons.length)%buttons.length]?.focus();}
   else if(evt.key==='Tab')close();
  });
 }
 for(const target of [stage,layers]){target.addEventListener('contextmenu',evt=>open(evt));target.addEventListener('keydown',evt=>{if(evt.key==='ContextMenu'||evt.shiftKey&&evt.key==='F10')open(evt,true);});}
 document.addEventListener('pointerdown',evt=>{if(menu&&!menu.contains(evt.target))close();},true);
 window.addEventListener('blur',()=>close());stage.addEventListener('scroll',()=>close());
 return {close};
}
