// Isolated CDP target and synthetic, hardware-free music stream. Never attaches
// to an existing user page. Start headless Chrome with a separate profile first.
import fs from 'node:fs';
import assert from 'node:assert/strict';
const host=`http://127.0.0.1:${process.env.CDP_PORT||19224}`;
const target=await(await fetch(`${host}/json/new?about:blank`,{method:'PUT'})).json();
const ws=new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r=>ws.addEventListener('open',r,{once:true}));
let next=0;const pending=new Map();const errors=[];
ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(m.error):p.resolve(m.result);}else if(m.method==='Runtime.exceptionThrown')errors.push(m.params);};
const send=(method,params={})=>new Promise((resolve,reject)=>{const id=++next;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});
const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
try {
 await send('Runtime.enable');await send('Page.enable');
 await send('Emulation.setDeviceMetricsOverride',{width:1440,height:1000,deviceScaleFactor:1,mobile:false});
 await send('Page.addScriptToEvaluateOnNewDocument',{source:String.raw`(()=>{
 const nativeWS=window.WebSocket;window.__workers=[];window.__frames=[];window.__messageTimes=[];
 window.Worker=class extends Worker {constructor(...a){super(...a);window.__workers.push(this);this.addEventListener('message',e=>{if(e.data?.Stats){window.__frames.push({at:Date.now(),...e.data.Stats});}});}};
 const names=['bpm','harmony','bass','pulse',...Array.from({length:44},(_,i)=>'part'+i)];
 const lines=names.map(n=>n==='bpm'?'const bpm = 124;':n==='harmony'?'const harmony = chord("<Am9 Fmaj7 Cmaj7 G6>").dict("ireal");':'const '+n+' = note("c3 e3 g3 b3 c4 e4 g4 b4").s("gm_acoustic_guitar_nylon").gain(0.43);');
 const statement='$: stack(bass, pulse)';let source=lines.join('\n')+'\n'+statement;
 const bindings=names.map((name,i)=>{const start=lines.slice(0,i).reduce((n,l)=>n+l.length+1,0);return {name,from:start+6,to:start+6+name.length,definition_from:start,definition_to:start+lines[i].length,color:[100,200,240]};});
 const snapshot={version:1,session:'performance-fixture',generation:1,cycle:0,cps:0.5,source,bindings,transport:{paused:false},lanes:[{id:'#0',label:'$',source:statement,line:lines.length+1,transpose:0,parts:names.map(name=>({name,color:[100,200,240],helper:name==='bpm'||name==='harmony'}))}],events:[],rests:[]};
 snapshot.definitions=[{name:'bpm',color:[255,180,90],value:124,history:[[0,124],[1,124]],dependencies:[],histogram:{}},{name:'harmony',color:[100,200,240],value:null,history:[],dependencies:[],histogram:{C4:3,E4:2,G4:4}}];
 const frame=(topic,payload)=>{const t=new TextEncoder().encode(topic),p=new TextEncoder().encode(JSON.stringify(payload)),b=new Uint8Array(8+t.length+p.length),v=new DataView(b.buffer);v.setUint32(0,b.length-4);b[4]=5;v.setUint16(6,t.length);b.set(t,8);b.set(p,8+t.length);return b.buffer;};
 window.WebSocket=class extends EventTarget {constructor(url,...args){super();if(!url.includes(':9613'))return new nativeWS(url,...args);this.readyState=1;setTimeout(()=>this.onopen?.(new Event('open')),0);const started=performance.now();this.timer=setInterval(()=>{
 snapshot.cycle=(performance.now()-started)/2000;
 const seed=Math.floor(snapshot.cycle*8);
 snapshot.events=Array.from({length:512},(_,i)=>{const part=bindings[2+(i+seed)%46],pitch=48+(i+seed)%36;return {id:seed*512+i,lane:'#0',part:part.name,part_color:[100,200,240],cycle:snapshot.cycle-1+(i%32)/16,duration:0.125,pitch,original_pitch:pitch,sound:'gm_acoustic_guitar_nylon',gain:0.43,block_label:'gm_acoustic_guitar_nylon / MIDI '+pitch+' / struct x.gain(0.43)',context:[[part.from,part.to]],controls:{gain:0.43}};});
 const begin=performance.now();this.onmessage?.(new MessageEvent('message',{data:frame('music/snapshot',snapshot)}));window.__messageTimes.push(performance.now()-begin);
 },50);}send(){}close(){clearInterval(this.timer);this.readyState=3;}};
 })()`});
 await send('Page.navigate',{url:process.env.VIEWER_URL||'http://127.0.0.1:18765/?music=1'});
 for(let i=0;i<60;i++){if(await evaluate('window.__frames?.length>1'))break;await sleep(500);}
 assert(await evaluate('window.__frames?.length>1'),`GPU did not initialize: ${JSON.stringify(errors)}`);
 const definitions=await evaluate(`(()=>{const cards=[...document.querySelectorAll('button')].filter(b=>b.title.startsWith('Select definition'));return cards.map(b=>({text:b.textContent,graphs:b.querySelectorAll('svg').length}));})()`);
 assert.equal(definitions.length,2,'shared definitions were duplicated');assert(definitions.every(d=>d.graphs===1),'definition graphs missing');
 assert(definitions.some(d=>d.text.includes('bpm = 124')));assert(definitions.some(d=>d.text.includes('harmony')));
 await evaluate(`[...document.querySelectorAll('button')].find(b=>b.title.startsWith('Select definition')&&b.textContent.includes('harmony')).click()`);
 await sleep(1000);
 const selection=await evaluate(`(()=>{const p=document.querySelector('.music-source'),b=document.querySelector('[data-source-selection]');const n=[...p.querySelectorAll('span')].find(n=>n.textContent==='harmony');n.click();return {text:p.textContent.length,spans:p.querySelectorAll('span').length,border:getComputedStyle(b).borderTopWidth,fill:getComputedStyle(b).backgroundColor,nameFill:getComputedStyle(n).backgroundColor};})()`);
 assert.equal(selection.fill,'rgba(0, 0, 0, 0)');assert.equal(selection.nameFill,'rgba(0, 0, 0, 0)');
 assert.notEqual(selection.border,'0px');assert(selection.spans<selection.text/2);
 await evaluate(`window.__messageTimes=[];window.__frames=[];document.querySelector('.music-part-toggle').click()`);
 await sleep(8000);
 const result=await evaluate(`({messages:window.__messageTimes.length,worst:Math.max(...window.__messageTimes),mean:window.__messageTimes.reduce((a,b)=>a+b,0)/window.__messageTimes.length,frames:window.__frames.slice(-5),status:document.querySelector('[role=status]').textContent,mode:document.querySelector('[aria-label="Music view"]').value})`);
 console.log(JSON.stringify({selection,...result},null,2));
 assert(result.messages>30,'stream did not stay responsive');assert(result.worst<1000,'music message blocked the main thread for over a second');
 assert.equal(result.mode,'0','stage watchdog tripped');assert(result.frames.length>1,'worker stopped reporting frames');assert.equal(errors.length,0);
 const shot=await send('Page.captureScreenshot',{format:'png'});fs.writeFileSync('music-link/target/source-performance.png',Buffer.from(shot.data,'base64'));
 fs.writeFileSync('music-link/target/source-performance.json',JSON.stringify({selection,...result},null,2));
} finally {await send('Target.closeTarget',{targetId:target.id});ws.close();}
