// End-to-end check of the workbench, run by scripts/e2e.sh.
//
// Drives headless Chromium over the DevTools protocol against a real mythos-server and a real
// (simulated) Modbus TCP device: real elements are clicked and typed into, and what is asserted
// is what the page shows and what the controller reports. Nothing is stubbed.
//
// usage: node workbench.mjs <screenshot dir> <server base URL> <simulator port> [chromium]
import { spawn } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [outDir, base, simPort, chromium = 'chromium'] = process.argv.slice(2);
if (!outDir || !base || !simPort) {
  console.error('usage: node workbench.mjs <screenshot dir> <server base URL> <simulator port> [chromium]');
  process.exit(2);
}
const port = 9400 + Math.floor(Math.random() * 400);
const chrome = spawn(chromium, ['--headless=new', '--no-sandbox', '--disable-gpu', '--hide-scrollbars',
  `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), 'e2e-'))}`,
  '--window-size=1440,900', 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function http(path, init) {
  for (let i = 0; i < 50; i++) { try { return await fetch(`http://127.0.0.1:${port}${path}`, init); } catch { await sleep(200); } }
  throw new Error('chromium did not start');
}
let failures = 0;
const check = (name, ok, detail = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '  — ' + detail : ''}`); if (!ok) failures++; };

try {
  await http('/json/version');
  const tab = await (await http(`/json/new?${encodeURIComponent(base + '/#/project/plant_a/devices')}`, { method: 'PUT' })).json();
  const ws = new WebSocket(tab.webSocketDebuggerUrl);
  let id = 0; const pending = new Map();
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
  await new Promise((r) => { ws.onopen = r; });
  const send = (method, params = {}) => new Promise((r) => { const n = ++id; pending.set(n, r); ws.send(JSON.stringify({ id: n, method, params })); });
  const evaluate = async (expression) => {
    const m = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (m.result?.exceptionDetails) throw new Error(m.result.exceptionDetails.exception?.description ?? 'page error');
    return m.result?.result?.value;
  };
  const shot = async (name) => { const m = await send('Page.captureScreenshot', { format: 'png' }); writeFileSync(join(outDir, `${name}.png`), Buffer.from(m.result.data, 'base64')); };
  await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
  await send('Page.enable');
  await sleep(2500);

  // Helpers that live in the page. `type` sets the value the way a keystroke does for React.
  await evaluate(`
    window.__t = {
      click(text) {
        const b = [...document.querySelectorAll('button')].find((el) => el.textContent.trim().includes(text));
        if (!b) throw new Error('no button: ' + text);
        if (b.disabled) throw new Error('button is disabled: ' + text);
        b.click();
      },
      type(label, value, index = 0) {
        const el = document.querySelectorAll('[aria-label="' + label + '"]')[index];
        if (!el) throw new Error('no field: ' + label);
        const proto = el.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
        Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value);
        el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true }));
      },
      dblclick(testId, index = 0) {
        const el = document.querySelectorAll('[data-testid="' + testId + '"]')[index];
        if (!el) throw new Error('no element: ' + testId);
        el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
      },
      text(sel) { return [...document.querySelectorAll(sel)].map((el) => el.textContent.trim()); },
      has(sel) { return document.querySelector(sel) !== null; },
    }; true`);

  check('the project opens on Devices & networks with nothing configured',
    await evaluate(`__t.has('[data-testid="no-connections"]')`));

  // 1. Add a connection and try to save it empty.
  await evaluate(`__t.click('Add connection')`); await sleep(300);
  check('adding a connection opens its configuration editor', await evaluate(`__t.has('[data-testid="connection-editor"]')`));
  await evaluate(`__t.click('Save configuration')`); await sleep(400);
  const errs = await evaluate(`__t.text('.field__error')`);
  check('saving it empty is refused in the form, field by field', errs.includes('A name is required') && errs.includes('A host is required'), errs.join(' | '));
  await shot('e2e_1_field_errors');

  // 2. Fill it in against the simulator, with one tag the device does not have.
  await evaluate(`
    __t.type('Name', 'boiler-plc'); __t.type('Poll interval (ms)', '250');
    __t.type('Host', '127.0.0.1'); __t.type('Port', '${simPort}');
    __t.type('Tag', 'external.BoilerHouse.Unit1.runtime_s'); __t.type('Address', '0'); true`);
  await evaluate(`__t.click('Add tag')`); await sleep(150);
  await evaluate(`__t.type('Tag', 'external.BoilerHouse.Unit1.steam_temp', 1); __t.type('Address', '2', 1); __t.type('Data type', 'f32', 1); true`);
  await evaluate(`__t.click('Add tag')`); await sleep(150);
  await evaluate(`__t.type('Tag', 'external.BoilerHouse.Unit1.feed_flow', 2); __t.type('Address', '99', 2); true`);
  await sleep(200);
  check('the draft is marked unsaved', await evaluate(`__t.has('[data-testid="unsaved"]')`));
  const leftover = await evaluate(`__t.text('.field__error')`);
  check('no field error is left beside a field that has been filled in', leftover.length === 0, leftover.join(' | '));
  await shot('e2e_2_filled_form');

  // 3. A configuration the controller refuses: the same tag twice.
  await evaluate(`__t.type('Tag', 'external.BoilerHouse.Unit1.runtime_s', 2)`);
  await evaluate(`__t.click('Save configuration')`); await sleep(900);
  const refused = await evaluate(`__t.text('[data-testid="server-problems"]').join(' ')`);
  check('a tag mapped twice is refused by the controller and the reason is shown', /mapped more than once/.test(refused), refused.slice(0, 160));
  check('nothing was saved on the controller', (await (await fetch(base + '/api/v1/projects/plant_a/io')).json()).config === null);
  check('the reason is shown once, not twice', (await evaluate(`document.querySelectorAll('.notice--bad').length`)) === 1);
  await shot('e2e_3_refused_by_controller');

  // 4. Fix it and save.
  await evaluate(`__t.type('Tag', 'external.BoilerHouse.Unit1.feed_flow', 2)`);
  await evaluate(`__t.click('Save configuration')`); await sleep(2500);
  check('after a successful save the draft is no longer unsaved', !(await evaluate(`__t.has('[data-testid="unsaved"]')`)));
  const stored = (await (await fetch(base + '/api/v1/projects/plant_a/io')).json()).config;
  check('the controller stored the connection with its three tags', stored?.connections?.[0]?.id === 'boiler-plc' && stored.connections[0].config.mappings.length === 3);
  const status = await (await fetch(base + '/api/v1/io/status')).json();
  check('the controller is running it, without a restart', status.connections?.[0]?.state === 'RUNNING' && status.connections[0].tags.length === 3, JSON.stringify(status.connections?.[0]?.state));

  // 5. The tree now has runtime nodes; open the live tag table from it.
  await sleep(2500);
  check('the tree shows the connection’s live tags node', await evaluate(`__t.has('[data-testid="tree-runtime-tags"]')`));
  await evaluate(`__t.dblclick('tree-runtime-tags')`); await sleep(2500);
  const rows = await evaluate(`[...document.querySelectorAll('[data-testid="tag-row"]')].map((tr) => ({
    uri: tr.getAttribute('data-uri'),
    state: tr.querySelector('[data-testid="tag-value"]').getAttribute('data-state'),
    value: tr.querySelector('[data-testid="tag-value"]').textContent,
    badge: tr.querySelector('[data-testid="quality-badge"]').textContent }))`);
  const by = Object.fromEntries(rows.map((r) => [r.uri.split('.').pop(), r]));
  check('runtime_s is live with a whole number', by.runtime_s?.state === 'live' && /^\d+$/.test(by.runtime_s.value), JSON.stringify(by.runtime_s));
  check('steam_temp is live with a REAL in the simulator’s range', by.steam_temp?.state === 'live' && Number(by.steam_temp.value) >= 20 && Number(by.steam_temp.value) <= 80, JSON.stringify(by.steam_temp));
  check('feed_flow (a register the device lacks) has no value and says CONFIG ERROR', by.feed_flow?.state === 'never-read' && by.feed_flow.value === '—' && by.feed_flow.badge === 'BAD CONFIG ERROR', JSON.stringify(by.feed_flow));
  const first = by.runtime_s?.value; await sleep(2200);
  const later = await evaluate(`document.querySelector('[data-uri="external.BoilerHouse.Unit1.runtime_s"] [data-testid="tag-value"]').textContent`);
  check('the value on screen follows the device', later !== first, `${first} -> ${later}`);
  await shot('e2e_4_live_tags_after_save');

  // 6. Select a tag: its properties appear in the inspector.
  await evaluate(`document.querySelector('[data-uri="external.BoilerHouse.Unit1.steam_temp"]').click()`); await sleep(300);
  const props = await evaluate(`__t.text('[data-testid="tag-properties"]').join(' ')`);
  check('selecting a tag shows its properties, including where it is read from', /holding register 2/.test(props) && /REAL/.test(props), props.slice(0, 200));

  // 7. Go offline: values disappear.
  await evaluate(`__t.click('Go offline')`); await sleep(500);
  const offline = await evaluate(`__t.text('[data-testid="tag-value"]')`);
  // Three table rows plus the selected tag's value in the inspector.
  check('going offline removes every value from the screen, table and inspector alike', offline.length === 4 && offline.every((v) => v === '—'), offline.join(','));
  await shot('e2e_5_offline');
  ws.close();
} catch (e) {
  console.log('FAIL  the script itself failed —', e.message); failures++;
} finally {
  chrome.kill('SIGKILL');
}
console.log(failures === 0 ? '\nALL CHECKS PASSED' : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
