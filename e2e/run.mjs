// Tests the full phone-to-computer video path using a headless browser with a fake camera.
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import puppeteer from "puppeteer-core";

const root = new URL("..", import.meta.url).pathname;
const dataDir = mkdtempSync(join(tmpdir(), "opencamera-e2e-"));
const port = 47900 + Math.floor(Math.random() * 50);
const engine = spawn(join(process.env.CARGO_TARGET_DIR ?? join(root, "target"), "debug/examples/headless"), [dataDir, String(port)], { stdio: ["pipe", "pipe", "inherit"] });

let state = {};
let stats = {};
let pairing = null;
createInterface({ input: engine.stdout }).on("line", (line) => {
  const [kind, ...rest] = line.split(" ");
  const data = JSON.parse(rest.join(" "));
  if (kind === "STATE") state = data;
  if (kind === "STATS") stats = data;
  if (kind === "PAIRING") pairing = data;
});

function send(cmd) {
  engine.stdin.write(cmd + "\n");
}

async function waitFor(what, fn, timeout = 20000) {
  const start = Date.now();
  while (!fn()) {
    if (Date.now() - start > timeout) throw new Error(`timed out waiting for ${what}; state=${JSON.stringify(state)} stats=${JSON.stringify(stats)}`);
    await new Promise((r) => setTimeout(r, 100));
  }
  console.log(`✓ ${what}`);
}

function clickText(page, text) {
  return page.waitForFunction(
    (t) => {
      const button = [...document.querySelectorAll("button")].find((b) => b.textContent.includes(t));
      button?.click();
      return !!button;
    },
    { timeout: 10000 },
    text,
  );
}

function waitText(page, text) {
  return page.waitForFunction((t) => document.body.innerText.includes(t), { timeout: 15000 }, text);
}

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME_PATH ?? "/usr/bin/chromium",
  headless: true,
  args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream", "--ignore-certificate-errors", "--no-sandbox"],
});

try {
  send("pair");
  await waitFor("pairing link issued", () => pairing);
  const page = await browser.newPage();
  await page.goto(pairing.url);
  await clickText(page, "Start camera");
  await waitFor("phone connected", () => state.phase === "connected");
  await waitFor("frames decoded at 1080p", () => stats.fps > 5 && stats.width === 1920);
  await waitFor("phone state relayed", () => state.phone?.settings?.resolution === "1080p");

  send('control {"resolution":"720p"}');
  await waitFor("desktop resolution change applied on phone", () => stats.width === 1280);

  await waitFor("phone streams H.264 Baseline", () => stats.phone?.profile === "baseline" && stats.fps > 5);
  await waitFor("encoder info relayed", () => stats.phone?.encoder && stats.phone?.scale >= 1);
  await waitFor("phone power state relayed", () => state.phone?.power !== undefined);
  send('control {"bitrate":1500}');
  await waitFor("manual bitrate applied", () => state.phone?.settings?.bitrate === 1500 && stats.fps > 5);
  send('control {"bitrate":"auto"}');

  const replay = await browser.newPage();
  await replay.goto(pairing.url);
  await clickText(replay, "Start camera");
  await waitText(replay, "no longer valid");
  console.log("✓ pairing token is single-use");
  await replay.close();
  await waitFor("original session unaffected", () => state.phase === "connected" && stats.fps > 5);

  send("disconnect");
  await waitText(page, "Your computer ended the connection");
  await waitFor("desktop disconnect", () => state.phase === "disconnected");
  await clickText(page, "Reconnect");
  await waitFor("reconnected with stored device credentials", () => state.phase === "connected");
  await waitFor("frames after reconnect", () => stats.fps > 5);

  await page.click('[aria-label="Disconnect"]');
  await waitFor("phone-initiated disconnect", () => state.phase === "disconnected");
  console.log("\nAll end-to-end checks passed.");
} catch (e) {
  console.error(e.message);
  process.exitCode = 1;
} finally {
  await browser.close();
  send("quit");
  engine.kill();
  rmSync(dataDir, { recursive: true, force: true });
}
