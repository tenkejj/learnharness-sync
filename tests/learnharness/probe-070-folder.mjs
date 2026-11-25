// Only run in an isolated disposable LearnHarness 0.7.0 profile.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { chromium } from "/node_modules/playwright/index.mjs";

const browser = await chromium.connectOverCDP("http://127.0.0.1:9222");
const deadline = Date.now() + 45_000;
let page;
while (Date.now() < deadline) {
  page = browser.contexts()[0].pages().find((candidate) => /^http:\/\/127\.0\.0\.1:/.test(candidate.url()));
  if (page) break;
  await new Promise((resolve) => setTimeout(resolve, 250));
}
assert.ok(page, "LearnHarness 0.7.0 window did not load");
const chatId = readFileSync("/profile/transcript-id", "utf8").trim();
const request = (action) => page.evaluate(async (body) => {
  const response = await fetch("/api/graph/sources", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) throw Error(`Source organization ${response.status}`);
  return response.json();
}, action);
const created = await request({ action: "create-folder", name: "Disposable 0.7 folder", parentId: null });
const folder = created.organization.folders.find((candidate) => candidate.name === "Disposable 0.7 folder");
assert.ok(folder?.id);
const moved = await request({ action: "move", kind: "source", id: chatId, parentId: folder.id });
assert.equal(moved.organization.placements[chatId].folderId, folder.id);
console.log("LearnHarness 0.7.0 persisted a folder and conversation placement");
await page.evaluate(() => setTimeout(() => window.close(), 10));
await browser.close();
