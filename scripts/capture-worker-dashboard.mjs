// Optional browser evidence only: Node built-ins and a private Chromium CDP pipe.
// The Python harness owns the whole process group and its ten-second deadline.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {writeFile} from 'node:fs/promises';
import path from 'node:path';

const [browser, url, profile, output] = process.argv.slice(2);
assert(browser && url && profile && output, 'browser, URL, profile and output required');
assert.equal(new URL(url).hostname, '127.0.0.1');
const chrome = spawn(browser, [
  '--headless', '--no-sandbox', '--disable-gpu', '--disable-background-networking',
  '--no-first-run', '--hide-scrollbars', '--remote-debugging-pipe',
  `--user-data-dir=${profile}`, 'about:blank',
], {stdio: ['ignore', 'ignore', 'ignore', 'pipe', 'pipe']});
let sequence = 0;
let buffer = Buffer.alloc(0);
const pending = new Map();
function rejectPending(error) {
  for (const request of pending.values()) request.reject(error);
  pending.clear();
}
chrome.on('error', rejectPending);
chrome.on('exit', () => rejectPending(new Error('browser exited before CDP response')));
chrome.stdio[3].on('error', rejectPending);
chrome.stdio[4].on('data', chunk => {
  buffer = Buffer.concat([buffer, chunk]);
  assert(buffer.length <= 8 * 1024 * 1024, 'CDP response budget');
  let end;
  while ((end = buffer.indexOf(0)) !== -1) {
    const response = JSON.parse(buffer.subarray(0, end).toString('utf8'));
    buffer = buffer.subarray(end + 1);
    const request = pending.get(response.id);
    if (!request) continue;
    pending.delete(response.id);
    if (response.error) request.reject(new Error('CDP command failed'));
    else request.resolve(response.result);
  }
});
function command(method, params = {}, sessionId) {
  return new Promise((resolve, reject) => {
    const id = ++sequence;
    pending.set(id, {resolve, reject});
    chrome.stdio[3].write(JSON.stringify({id, method, params, sessionId}) + '\0');
  });
}
try {
  const {targetId} = await command('Target.createTarget', {url: 'about:blank'});
  const {sessionId} = await command('Target.attachToTarget', {targetId, flatten: true});
  const page = (method, params) => command(method, params, sessionId);
  await page('Page.enable');
  for (const [name, width, height, fragment] of [
    ['desktop', 1280, 1800, ''], ['mobile', 390, 1000, ''],
    ['mobile-health', 390, 1000, '#health'],
  ]) {
    await page('Emulation.setDeviceMetricsOverride', {width, height, deviceScaleFactor: 1, mobile: false});
    const navigation = await page('Page.navigate', {url: url + fragment});
    assert(!navigation.errorText, 'dashboard navigation failed');
    // Poll boundedly through CDP; no scripts or assets are added to the page.
    let ready = false;
    for (let trial = 0; trial < 100; trial++) {
      const result = await page('Runtime.evaluate', {expression:
        `document.readyState === 'complete' && location.href === ${JSON.stringify(url + fragment)} && document.querySelectorAll('[data-panel]').length === 37`,
        returnByValue: true});
      if (result.result.value) { ready = true; break; }
      await new Promise(resolve => setTimeout(resolve, 20));
    }
    assert(ready, 'dashboard DOM readiness deadline');
    const layout = await page('Runtime.evaluate', {expression: `new Promise(resolve => {
      ${fragment ? "document.querySelector('#health').scrollIntoView();" : 'scrollTo(0, 0);'}
      requestAnimationFrame(() => requestAnimationFrame(() => {
        const health = document.querySelector('#health').getBoundingClientRect();
        resolve({x: scrollX, y: scrollY, healthTop: health.top,
          healthWidth: health.width, viewport: innerWidth});
      }));
    })`, awaitPromise: true, returnByValue: true});
    assert(!layout.exceptionDetails, 'dashboard layout evaluation failed');
    const position = layout.result.value;
    assert.equal(position.viewport, width);
    if (fragment) {
      assert(Math.abs(position.healthTop) < 1, 'health anchor did not enter viewport');
      assert(position.healthWidth > 0 && position.healthWidth <= width, 'health section overflow');
    }
    const shot = await page('Page.captureScreenshot', {format: 'png', captureBeyondViewport: false,
      clip: {x: position.x, y: position.y, width, height, scale: 1}});
    const bytes = Buffer.from(shot.data, 'base64');
    assert(bytes.length > 1000 && bytes.length <= 4 * 1024 * 1024, 'screenshot byte budget');
    await writeFile(path.join(output, `${name}.png`), bytes, {flag: 'wx', mode: 0o600});
  }
  await command('Browser.close');
} finally {
  chrome.kill('SIGTERM');
}
