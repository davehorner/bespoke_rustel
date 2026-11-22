import assert from 'node:assert/strict';
const url='http://127.0.0.1:9614';
const read=async()=>await(await fetch(url+'/sessions')).json();
const snapshot=(await read()).find(s=>s.lanes.length>=5);assert(snapshot,'Need a running demo');
const bytes=Buffer.from(snapshot.source,'utf8');assert(bytes.length>0);
const ranged=snapshot.events.filter(e=>e.context?.length);assert(ranged.length>0,'Missing source ranges');
const tokens=[];
for(const e of ranged){assert(e.context.length<=64);for(const [a,b] of e.context){assert(Number.isInteger(a)&&Number.isInteger(b)&&0<=a&&a<=b&&b<=bytes.length);tokens.push(bytes.subarray(a,b).toString('utf8'));}}
assert(tokens.some(t=>/^[a-g][#b]?[0-9]$/.test(t)),'No exact note tokens');
assert(snapshot.events.some(e=>Number.isFinite(e.controls?.pan)),'Missing resolved control samples');
const lane=snapshot.lanes[4];const previous=lane.transpose;
async function transpose(semitones){const r=await fetch(url+'/transpose',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({request:crypto.randomUUID(),session:snapshot.session,generation:snapshot.generation,lane:lane.id,semitones})});assert.equal(r.status,200);assert((await r.json()).accepted);}
try {
 await transpose(7);let found=false;
 for(let i=0;i<100;i++){const current=(await read()).find(s=>s.session===snapshot.session);if(current?.events.some(e=>e.lane===lane.id&&e.pitch!==null&&e.original_pitch!==null&&Math.abs(e.pitch-e.original_pitch-7)<1e-8)){found=true;break;}await new Promise(r=>setTimeout(r,100));}
 assert(found,'Transpose did not reach visual event data');
 console.log(`PASS: ${snapshot.lanes.length} lanes, ${ranged.length} events with bounded byte ranges, resolved controls and +7 transposed event pitches. Wire protocol v${snapshot.version} unchanged.`);
} finally {await transpose(previous);}
