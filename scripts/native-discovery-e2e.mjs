// Real multicast discovery between two isolated native processes on this host.
import { spawn } from "node:child_process";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
const root = await mkdtemp(join(tmpdir(), "harnesssync-mdns-"));
const binary = resolve(process.argv[2] || "target/debug/harness-sync-fixture-peer");
const peers = [];
try {
  for (const name of ["LAN-Desktop", "LAN-Laptop"]) {
    const child = spawn(binary, ["--synthetic-only", "--lan", "--profile", join(root, name), "--name", name], {stdio:["pipe","pipe","inherit"]});
    const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
    await lines.next();
    peers.push({ child, async status() { child.stdin.write(JSON.stringify({command:"status"})+"\n"); const next = await lines.next(); if(next.done)throw Error("Native peer stopped"); return JSON.parse(next.value).result; } });
  }
  let found = false;
  for (let i = 0; i < 50; i++) {
    const states = await Promise.all(peers.map(p => p.status()));
    if (states.every((s, i) => s.discovered.some(d => d.id === states[1-i].transport.device_id))) {
      console.log(JSON.stringify({discovery:true,devices:states.map((s,i)=>({name:s.device,nearby:[states[1-i].device]})),physical_devices:false},null,2));
      found = true; break;
    }
    await new Promise(resolve => setTimeout(resolve, 200));
  }
  if (!found) throw Error("LAN discovery not observed on this host; physical-network testing is still required");
} finally {
  for (const {child} of peers) child.stdin.end(JSON.stringify({command:"exit"})+"\n");
}
