import { peerLab } from "./native-peer-lab.mjs";
import { mkdtemp, readFile, realpath, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import assert from "node:assert/strict";
const root = await mkdtemp(join(await realpath(tmpdir()), "harnesssync-two-process-"));
const {start, wait, pair, close} = peerLab(root, resolve(process.argv[2] || "target/debug/harness-sync-fixture-peer"));
try {
  let a=await start("Desktop"),b=await start("Laptop");const identities=[a.identity,b.identity];assert.notEqual(...identities);
  await a.request("initialize_synthetic",{confirmed:true});await b.request("initialize_synthetic",{confirmed:true});
  const first=(await a.request("synthetic_edit",{text:"Invented desktop note"})).snapshot;
  await pair(a,b);await wait(b,s=>s.head===first);
  await wait(a,s=>s.transport.peers[0]?.connected);
  const test=await a.request("run_sync_test",{device:b.identity});assert.equal(test.state,"success");
  const receipt=(await a.request("status")).transport.peers[0].receipt;
  assert.equal(receipt.head,test.snapshot);assert.equal(receipt.received,test.snapshot);
  assert.equal((await b.request("status")).head,test.snapshot);
  const second=(await b.request("synthetic_edit",{text:"Invented laptop note"})).snapshot;await wait(a,s=>s.head===second);
  await b.stop();const forward=(await a.request("synthetic_edit",{text:"Offline desktop continuation"})).snapshot;
  b=await start("Laptop");assert.equal(b.identity,identities[1]);await wait(b,s=>s.head===forward);
  await a.stop();await b.stop();
  a=await start("Desktop");assert.equal(a.identity,identities[0]);const left=(await a.request("synthetic_edit",{text:"Offline left branch"})).snapshot;await a.stop();
  b=await start("Laptop");const right=(await b.request("synthetic_edit",{text:"Offline right branch"})).snapshot;
  a=await start("Desktop");await wait(a,s=>s.conflicts.includes(right));await wait(b,s=>s.conflicts.includes(left));assert.equal((await a.request("status")).head,left);assert.equal((await b.request("status")).head,right);
  for(const name of ["Desktop","Laptop"]){const saved=JSON.parse(await readFile(join(root,name,"devices.json")));assert.equal(Object.keys(saved.peers).length,1);}
  await a.stop();await b.stop();
  const report={passed:true,transport:"real QUIC over loopback",processes:2,identities_persisted:true,mutual_pairing:true,verified_one_click_test:true,bidirectional_transfer:true,restart_reconnect:true,offline_fast_forward:true,divergence_preserved:true,production_data_access:false};
  const path=join(root,"report.json");await writeFile(path,JSON.stringify(report,null,2));console.log(JSON.stringify({...report,report:path},null,2));
} finally {close();}
