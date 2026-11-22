import fs from 'node:fs';
import assert from 'node:assert/strict';
const out='music-link/contribution/screenshots';fs.mkdirSync(out,{recursive:true});
const target=await(await fetch('http://127.0.0.1:19222/json/new?about:blank',{method:'PUT'})).json();
const ws=new WebSocket(target.webSocketDebuggerUrl);await new Promise(r=>ws.addEventListener('open',r,{once:true}));
let id=0;const pending=new Map();const errors=[];
ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p?.reject(m.error):p?.resolve(m.result);}else if(m.method==='Runtime.exceptionThrown')errors.push(m);};
const send=(method,params={})=>new Promise((resolve,reject)=>{const next=++id;pending.set(next,{resolve,reject});ws.send(JSON.stringify({id:next,method,params}));});
const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,returnByValue:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
const wait=ms=>new Promise(r=>setTimeout(r,ms));
const capture=async name=>{const r=await send('Page.captureScreenshot',{format:'png'});fs.writeFileSync(`${out}/${name}.png`,Buffer.from(r.data,'base64'));};
let soloLane, session, originalSolo;
try {
 await send('Runtime.enable');await send('Page.enable');await send('Emulation.setDeviceMetricsOverride',{width:1600,height:1000,deviceScaleFactor:1,mobile:false});
 await send('Page.navigate',{url:'http://127.0.0.1:18765/?music=1&capture=upstream'});
 for(let i=0;i<100;i++){if(await evaluate(`document.querySelector('[aria-label="Lane filter"]')?.options.length>3`))break;await wait(100);}
 for(let i=0;i<120;i++){if(await evaluate(`!document.body.innerText.includes('Starting the renderer')`))break;await wait(500);}
 assert(await evaluate(`!document.body.innerText.includes('Starting the renderer')`),'Renderer did not finish startup');
 await evaluate(`(()=>{const s=document.querySelector('[aria-label="Music view"]');s.value='0';s.dispatchEvent(new Event('change',{bubbles:true}));})()`);
 await send('Page.bringToFront');await wait(2500);assert(await evaluate(`document.querySelector('[aria-label="Lane filter"]').options.length>3`));
 await send('Input.dispatchMouseEvent',{type:'mouseWheel',x:1050,y:350,deltaX:0,deltaY:850});await wait(1500);
 await capture('nightshade-stage');
 const names=['stage','piano-roll','wordfall','spiral','pitch-wheel','scope','spectrum','vectorscope','control-curves'];
 for(let mode=1;mode<=8;mode++) { await evaluate(`(()=>{const s=document.querySelector('[aria-label="Music view"]');s.value='${mode}';s.dispatchEvent(new Event('change',{bubbles:true}));})()`);await wait(700);await capture(`nightshade-${names[mode]}`); }
 // The current source panel and actual solo checkbox, not a mockup.
 await evaluate(`(()=>{const s=document.querySelector('[aria-label="Music view"]');s.value='1';s.dispatchEvent(new Event('change',{bubbles:true}));const l=document.querySelector('[aria-label="Lane filter"]');l.value=l.options[4].value;l.dispatchEvent(new Event('change',{bubbles:true}));})()`);await wait(200);
 const sessions=await(await fetch('http://127.0.0.1:9614/sessions')).json();session=sessions[0];originalSolo=session.solo_lane;soloLane=await evaluate(`document.querySelector('[aria-label="Lane filter"]').value`);
 await evaluate(`(()=>{const solo=document.querySelector('[aria-label="Solo selected lane"]');if(!solo.checked)solo.click();})()`);await wait(700);
 assert((await(await fetch('http://127.0.0.1:9614/sessions')).json()).some(s=>s.session===session.session&&s.solo_lane===soloLane));
 await evaluate(`document.querySelectorAll('.music-lane details')[3].open=true`);await capture('nightshade-solo');
 assert.equal(errors.length,0);fs.writeFileSync('music-link/contribution/runtime/browser-capture.json',JSON.stringify({captured:new Date().toISOString(),session:session.session,views:names,soloLane,exceptions:errors.length},null,2));
 console.log('Captured nine views and Solo with current controls.');
} finally {
 if(soloLane&&session)await fetch('http://127.0.0.1:9614/solo',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({request:crypto.randomUUID(),session:session.session,generation:session.generation,lane:originalSolo || soloLane,solo:Boolean(originalSolo)})});
 ws.close();await fetch(`http://127.0.0.1:19222/json/close/${target.id}`);
}
