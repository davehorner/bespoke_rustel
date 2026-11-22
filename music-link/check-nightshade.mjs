import fs from 'node:fs';
import assert from 'node:assert/strict';
const port = process.env.CDP_PORT || '19222';
const url = process.env.VIEWER_URL || 'http://127.0.0.1:18765/?music=1';
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const target = targets.find(t => t.type === 'page');
assert(target, 'No headless page target');
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r => ws.addEventListener('open', r, {once:true}));
let next=0; const pending=new Map(); const errors=[];
function send(method, params={}, sessionId) { return new Promise((resolve,reject)=>{ const id=++next;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params,...(sessionId?{sessionId}:{})}));}); }
ws.addEventListener('message', async e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p?.reject(m.error):p?.resolve(m.result);return;} if(m.method==='Target.attachedToTarget'){await send('Runtime.enable',{},m.params.sessionId);await send('Runtime.runIfWaitingForDebugger',{},m.params.sessionId);}if(m.method==='Runtime.exceptionThrown' || m.method==='Inspector.targetCrashed') { errors.push(m);console.log(JSON.stringify(m).slice(0,6000)); }});
const sleep = ms => new Promise(r=>setTimeout(r,ms));
async function evaluate(expression) { const r=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true}); if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails)); return r.result.value; }
await send('Runtime.enable');await send('Page.enable');await send('Target.setAutoAttach',{autoAttach:true,waitForDebuggerOnStart:true,flatten:true});
await send('Emulation.setDeviceMetricsOverride',{width:1280,height:900,deviceScaleFactor:1,mobile:false});
await send('Page.addScriptToEvaluateOnNewDocument',{source:`window.__workers=[]; window.__frames=[]; window.Worker=class extends Worker { constructor(...args) { super(...args); window.__workers.push(this); this.addEventListener('message',e=>{ if(e.data?.Stats) { window.__frames.push({at:Date.now(),...e.data.Stats}); if(window.__frames.length>100)window.__frames.shift(); }}); } postMessage(data,...args) { if(data.message?.MusicSnapshot){window.__lastMusic=data; if(window.__stress)return;} return super.postMessage(data,...args); } sendTest(data) { return super.postMessage(data); }};`});
await send('Page.navigate',{url});
for(let i=0;i<40;i++){await sleep(500);if(await evaluate(`!!document.querySelector('[aria-label="Music view"]') && window.__frames.length>1`))break;}
console.log('Startup:',await evaluate(`({frames:window.__frames.length, status:document.querySelector('[role="status"]')?.textContent})`));
assert(await evaluate(`window.__frames.length>1`),'GPU worker did not render');
const modes=await evaluate(`Array.from(document.querySelector('[aria-label="Music view"]').options).map(o=>o.textContent)`);
assert.equal(modes.length,9);console.log('Modes:',modes);
const results=[];
for(let mode=1;mode<=8;mode++){
 await evaluate(`(()=>{const s=document.querySelector('[aria-label="Music view"]');s.value='${mode}';s.dispatchEvent(new Event('change',{bubbles:true}));})()`);
 await sleep(1200);
 const result=await evaluate(`(()=>{const c=document.querySelector('[aria-label="Music visualization"]');const d=c.getContext('2d').getImageData(0,150,c.width,Math.max(1,c.height-300)).data;let colored=0;for(let i=0;i<d.length;i+=4){if(d[i]>30||d[i+1]>40||d[i+2]>50)colored++;}return {mode:document.querySelector('[aria-label="Music view"]').value,colored,display:getComputedStyle(c).display};})()`);
 assert.equal(result.mode,String(mode));assert.equal(result.display,'block');assert(result.colored>50,`Mode ${mode} did not draw`);
 results.push(result);
 const shot=await send('Page.captureScreenshot',{format:'png'});fs.writeFileSync(`music-link/nightshade-view-${mode}.png`,Buffer.from(shot.data,'base64'));
}
console.log('Rendered:',results);
// Return to the GPU view; it must resume after being paused in the flat views.
await evaluate(`(()=>{const s=document.querySelector('[aria-label="Music view"]');s.value='0';s.dispatchEvent(new Event('change',{bubbles:true}));})()`);
await sleep(2000);
assert(await evaluate(`Date.now()-window.__frames.at(-1).at<2000`),'Stage failed to resume');
// Stress the renderer at the protocol limit: 64 lanes and 2,048 events.
await evaluate(`(()=>{ window.__stress=true; const base=structuredClone(window.__lastMusic.message.MusicSnapshot); const start=Date.now(); window.__stressTimer=setInterval(()=>{const cycle=base.cycle+(Date.now()-start)/1000*base.cps; const lanes=Array.from({length:64},(_,i)=>({...base.lanes[0],id:'stress'+i}));const events=Array.from({length:2048},(_,i)=>({...base.events[0],id:i,lane:lanes[i%64].id,cycle:cycle+(i%32)/16-1,duration:0.125,pitch:36+i%48,original_pitch:36+i%48}));window.__workers[0].sendTest({message:{MusicSnapshot:{...base,cycle,lanes,events}}});},50); })()`);
await sleep(15000);
assert.equal(await evaluate(`document.querySelector('[aria-label="Music view"]').value`),'0','Dense score stopped the GPU worker');
assert(await evaluate(`Date.now()-window.__frames.at(-1).at<2500`),'Dense score stopped frame updates');
console.log('Dense score:',await evaluate(`window.__frames.slice(-3)`));
await evaluate(`clearInterval(window.__stressTimer);window.__stress=false;`);await sleep(1000);
// Force exactly the reported failure: GPU worker stops, page remains alive.
await evaluate(`window.__workers[0].terminate()`);await sleep(11500);
assert.equal(await evaluate(`document.querySelector('[aria-label="Music view"]').value`),'1','Freeze watchdog did not select the fallback');
assert.match(await evaluate(`document.querySelector('[role="status"]').textContent`),/stopped responding/);
assert.equal(errors.length,0,'Unexpected browser or worker exceptions');
fs.writeFileSync('music-link/nightshade-check-results.json',JSON.stringify({modes,results,freezeFallback:true,errors},null,2));
// Verify music mode does not require WebGPU for its flat/audio views.
await send('Page.addScriptToEvaluateOnNewDocument',{source:`Object.defineProperty(navigator,'gpu',{value:undefined,configurable:true});`});
await send('Page.navigate',{url});await sleep(3500);
assert.equal(await evaluate(`document.querySelector('[aria-label="Music view"]').value`),'1');
assert.equal(await evaluate(`window.__workers.length`),0);
assert.match(await evaluate(`document.querySelector('[role="status"]').textContent`),/WebGPU unavailable/);
assert.equal(errors.length,0);
console.log('PASS: all views draw; dense stage renders; frozen worker recovers; flat views work without WebGPU.');ws.close();
