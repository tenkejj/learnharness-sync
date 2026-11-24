(async () => {
  const invoke = window.__TAURI__.core.invoke;
  const cmd = (command, args = {}) => invoke("engine_request", { command, args });
  const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
  try {
    const before = await cmd("status");
    if (!/^[a-f0-9]{64}$/.test(before.transport.device_id)) throw Error("Native identity missing");
    if (before.transport.peers.length) throw Error("Fresh profile contains peers");
    await cmd("initialize_synthetic", { confirmed: true });
    const edit = await cmd("synthetic_edit", { text: "Invented native WebKit smoke note" });
    const after = await cmd("status");
    if (after.head !== edit.snapshot || !after.snapshots[0].verified) throw Error("Native snapshot unavailable");
    const note = await cmd("inspect_revision", { snapshot: edit.snapshot });
    if (note.text !== "Invented native WebKit smoke note") throw Error("Preserved note unavailable");
    await Promise.all(["production_import", "production_export", "run_sync_test"].map(async command => {
      let refused = false;
      try { await cmd(command, { device: "unpaired" }); } catch { refused = true; }
      if (!refused) throw Error(`${command} guard did not refuse`);
    }));
    await document.fonts.ready;
    if (!document.fonts.check("16px Manrope")) throw Error("Packaged font unavailable");
    await pause(800);
    const nav = [...document.querySelectorAll("nav button")];
    if (nav.length !== 3 || !nav.some(b => b.textContent === "Home")) throw Error("Simplified navigation unavailable");
    const devices = nav.find(b => b.textContent === "Devices");
    if (!devices) throw Error("Devices navigation missing");
    devices.click(); await pause(200);
    const add = [...document.querySelectorAll("button")].find(b => b.textContent === "Add Device");
    if (!add) throw Error("Native Devices UI unavailable");
    add.click(); await pause(300);
    if (!(await cmd("status")).pairing_open || !document.querySelector("dialog[open]")) throw Error("GUI Add Device did not open native pairing");
    document.querySelector("dialog").dispatchEvent(new Event("cancel", { cancelable: true })); await pause(200);
    if ((await cmd("status")).pairing_open || document.querySelector("dialog[open]")) throw Error("Closing discovery did not close new pairing requests");
    document.querySelector('button[aria-label="Settings"]').click(); await pause(200);
    if (!document.querySelector("dialog[open]")) throw Error("Settings dialog unavailable");
    if (!document.body.textContent.includes("Real LearnHarness synchronization is disabled.")) throw Error("Safety status missing");
    const installation = (await cmd("settings")).installation;
    for (let i = 0; i < 100 && !document.body.textContent.includes(installation); i++) await pause(50);
    if (!installation || !document.body.textContent.includes(installation)) throw Error("Installation status missing");
    if (window.__CHECK_TEST_BACKUP__) {
      const advanced = document.querySelector("details.advanced");
      advanced.open = true;
      const section = [...advanced.querySelectorAll("section")].find(e => e.textContent.includes("Check a LearnHarness backup"));
      const input = section.querySelector("input");
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, "/qa-backup");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      await pause(100);
      section.querySelector("button").click();
      for (let i = 0; i < 100 && !section.textContent.includes("Matches the tested format"); i++) await pause(50);
      if (!section.textContent.includes("1 session, 3 propositions and 2 chains")) throw Error("Generated backup inspection failed");
      let refused = false;
      try { await cmd("inspect_learnharness_backup", {path:"/qa-writable"}); } catch { refused = true; }
      if (!refused) throw Error("Writable profile accepted as a backup");
      const current = await cmd("status");
      if (current.head !== after.head || current.snapshots.length !== after.snapshots.length) throw Error("Inspection changed sync history");
    }
    await invoke("smoke_report", { report: { ok: true, backup_inspection: !!window.__CHECK_TEST_BACKUP__, installation, identity: before.transport.device_id, snapshot: after.head, native_commands: true, production_refused: true, offline_test_refused: true, preserved_note: true, navigation_destinations: 3, packaged_font: true, devices_ui: true, add_device_action: true, pairing_dismissal: true, settings_ui: true } });
  } catch (e) {
    await invoke("smoke_report", { report: { ok: false, error: String(e) } });
  }
})();
