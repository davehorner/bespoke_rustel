import assert from 'node:assert/strict';
const base='http://127.0.0.1:9614';
const snapshots=async()=>await(await fetch(base+'/sessions')).json();
const initial=(await snapshots())[0];assert(initial?.lanes.length>=3,'Need live lanes');
const command=async(lane,solo)=>{const r=await fetch(base+'/solo',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({request:crypto.randomUUID(),session:initial.session,generation:initial.generation,lane,solo})});assert.equal(r.status,200);assert((await r.json()).accepted);};
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function waitSolo(lane) {for(let i=0;i<50;i++){const s=(await snapshots()).find(s=>s.session===initial.session);if(s?.solo_lane===lane)return s;await delay(100);}throw Error('Solo failed to become '+lane);}
let ws, target;
try {
 await command(initial.lanes[0].id,true);await waitSolo(initial.lanes[0].id);
 await command(initial.lanes[1].id,true);await waitSolo(initial.lanes[1].id);
 await command(initial.lanes[1].id,false);await waitSolo(null);
 console.log('HTTP solo on / change / off passed');
 target=await(await fetch('http://127.0.0.1:19222/json/new?about:blank',{method:'PUT'})).json();
 ws=new WebSocket(target.webSocketDebuggerUrl);await new Promise(r=>ws.addEventListener('open',r,{once:true}));
 let next=0;const pending=new Map();const errors=[];
 ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p?.reject(m.error):p?.resolve(m.result);}else if(m.method==='Runtime.exceptionThrown')errors.push(m);});
 const send=(method,params={})=>new Promise((resolve,reject)=>{const id=++next;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});
 const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,returnByValue:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
 await send('Page.enable');await send('Runtime.enable');
 // Fast, isolated page test of the actual browser controls without a GPU dependency.
 await send('Page.addScriptToEvaluateOnNewDocument',{source:"Object.defineProperty(navigator,'gpu',{value:undefined,configurable:true});"});
 await send('Page.navigate',{url:'http://127.0.0.1:18765/?music=1&test=solo'});
 for(let i=0;i<80;i++){if(await evaluate(`document.querySelector('[aria-label="Lane filter"]')?.options.length>3`))break;await delay(100);}
 assert(await evaluate(`document.querySelector('[aria-label="Solo selected lane"]').disabled`));
 assert(!(await evaluate(`document.body.innerText.includes('SOURCE TRACES')`)));
 const choose=async lane=>await evaluate(`(()=>{const s=document.querySelector('[aria-label="Lane filter"]');s.value=${JSON.stringify(lane)};s.dispatchEvent(new Event('change',{bubbles:true}));})()`);
 await choose(initial.lanes[0].id);await delay(100);
 await evaluate(`document.querySelector('[aria-label="Solo selected lane"]').click()`);await waitSolo(initial.lanes[0].id);await delay(200);
 assert(await evaluate(`document.querySelector('[aria-label="Solo selected lane"]').checked`));
 await choose(initial.lanes[2].id);const solo=await waitSolo(initial.lanes[2].id);await delay(200);
 assert.equal(solo.lanes.length,initial.lanes.length,'Solo removed visual lanes');
 assert(new Set(solo.events.map(e=>e.lane)).size>1,'Solo removed other telemetry');
 assert.deepEqual(solo.lanes.map(l=>l.transpose),initial.lanes.map(l=>l.transpose),'Solo changed transpose');
 await evaluate(`document.querySelector('[aria-label="Solo selected lane"]').click()`);await waitSolo(null);
 await choose('');await delay(100);assert(await evaluate(`document.querySelector('[aria-label="Solo selected lane"]').disabled`));
 assert.equal(errors.length,0,'Browser exception');
 console.log('PASS: browser checkbox sends Solo; changing lane follows selection; off restores all; transpose and visualization preserved.');
} finally {
 const s=(await snapshots()).find(s=>s.session===initial.session);
 if(s?.solo_lane)await command(s.solo_lane,false);
 ws?.close();
 if(target)await fetch(`http://127.0.0.1:19222/json/close/${target.id}`);
}
