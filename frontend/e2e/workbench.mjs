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
// The account e2e.sh created: the browser signs in with it, and the checks made from here
// directly against the API carry its access token.
const { MYTHOS_E2E_USER: user, MYTHOS_E2E_PASSWORD: password, MYTHOS_E2E_TOKEN: token } = process.env;
if (!user || !password || !token) {
  console.error('workbench.mjs: MYTHOS_E2E_USER, MYTHOS_E2E_PASSWORD and MYTHOS_E2E_TOKEN are required');
  process.exit(2);
}
const api = (url, init = {}) =>
  fetch(url, { ...init, headers: { ...(init.headers ?? {}), Authorization: `Bearer ${token}` } });
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
      press(label, key) {
        const el = document.querySelector('[aria-label="' + label + '"]');
        if (!el) throw new Error('no field: ' + label);
        el.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
      },
      clickTestId(testId) {
        const el = document.querySelector('[data-testid="' + testId + '"]');
        if (!el) throw new Error('no element: ' + testId);
        el.click();
      },
      rect(sel) {
        const el = document.querySelector(sel);
        if (!el) throw new Error('no element: ' + sel);
        const r = el.getBoundingClientRect();
        return { x: r.x, y: r.y, width: r.width, height: r.height };
      },
      text(sel) { return [...document.querySelectorAll(sel)].map((el) => el.textContent.trim()); },
      has(sel) { return document.querySelector(sel) !== null; },
    }; true`);

  // Signing in. The address names a project, and nothing of it is shown until the controller
  // knows who is asking.
  check('the workbench asks to sign in before showing anything',
    await evaluate(`__t.has('[data-testid="sign-in"]') && !__t.has('[data-testid="connection-editor"]')`));
  await evaluate(`(() => {
    const set = (sel, value) => {
      const el = document.querySelector(sel);
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, value);
      el.dispatchEvent(new Event('input', { bubbles: true }));
    };
    set('input[autocomplete="username"]', ${JSON.stringify(user)});
    set('input[type="password"]', 'not the password');
    return true; })()`);
  await evaluate(`__t.click('Sign in')`); await sleep(800);
  check('a wrong password is refused', (await evaluate(`__t.text('[role="alert"]')`)).join(' ').includes('wrong password'));
  await evaluate(`(() => {
    const el = document.querySelector('input[type="password"]');
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, ${JSON.stringify(password)});
    el.dispatchEvent(new Event('input', { bubbles: true }));
    return true; })()`);
  await evaluate(`__t.click('Sign in')`); await sleep(2500);
  check('signing in opens the project the address names', !(await evaluate(`__t.has('[data-testid="sign-in"]')`)));

  check('the project opens on Devices & networks with nothing configured',
    await evaluate(`__t.has('[data-testid="no-connections"]')`));

  // 1. Add a connection and try to save it empty.
  await evaluate(`__t.click('Add connection')`); await sleep(300);
  check('adding a connection opens its configuration editor', await evaluate(`__t.has('[data-testid="connection-editor"]')`));
  await evaluate(`__t.click('Save configuration')`); await sleep(400);
  const errs = await evaluate(`__t.text('.field__error')`);
  check('saving it empty is refused in the form, field by field', errs.includes('A name is required') && errs.includes('Host is required'), errs.join(' | '));
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
  check('nothing was saved on the controller', (await (await api(base + '/api/v1/projects/plant_a/io')).json()).config === null);
  check('the reason is shown once, not twice', (await evaluate(`document.querySelectorAll('.notice--bad').length`)) === 1);
  await shot('e2e_3_refused_by_controller');

  // 4. Fix it and save.
  await evaluate(`__t.type('Tag', 'external.BoilerHouse.Unit1.feed_flow', 2)`);
  await evaluate(`__t.click('Save configuration')`); await sleep(2500);
  check('after a successful save the draft is no longer unsaved', !(await evaluate(`__t.has('[data-testid="unsaved"]')`)));
  const stored = (await (await api(base + '/api/v1/projects/plant_a/io')).json()).config;
  check('the controller stored the connection with its three tags', stored?.connections?.[0]?.id === 'boiler-plc' && stored.connections[0].config.mappings.length === 3);
  const status = await (await api(base + '/api/v1/io/status')).json();
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

  // 7. HMI screens: create one from the tree, place an I/O field, bind it, and watch it read.
  await evaluate(`__t.clickTestId('tree-add-screen')`); await sleep(200);
  await evaluate(`__t.type('Screen name', 'Boiler overview'); __t.press('Screen name', 'Enter'); true`); await sleep(900);
  check('a screen created from the tree opens in the screen editor', await evaluate(`__t.has('[data-testid="screen-editor"]')`));
  check('a new screen is reported as not released', (await evaluate(`__t.text('[data-testid="release-state"]')[0]`)) === 'NOT RELEASED');

  await evaluate(`__t.click('I/O field')`); await sleep(300);
  check('a new I/O field says NOT BOUND and shows no value',
    (await evaluate(`document.querySelector('[data-testid="io-field"]').getAttribute('data-state')`)) === 'not-bound');
  await evaluate(`__t.type('Tag', 'external.BoilerHouse.Unit1.steam_temp'); __t.type('Label', 'Steam temperature'); __t.type('Unit', '°C'); true`);
  await sleep(1200);
  const field = await evaluate(`(() => { const el = document.querySelector('[data-testid="io-field"]'); return {
    state: el.getAttribute('data-state'), value: el.querySelector('[data-testid="tag-value"]').textContent, text: el.textContent }; })()`);
  check('bound to a tag, the field shows the live value with its unit',
    field.state === 'live' && Number(field.value) >= 20 && Number(field.value) <= 80 && field.text.includes('°C') && field.text.includes('Steam temperature'),
    JSON.stringify(field));

  await evaluate(`__t.click('Text field')`); await sleep(200);
  await evaluate(`__t.type('Text', 'Boiler 1')`); await sleep(200);
  check('a text field shows what was typed', (await evaluate(`__t.text('.textfield')[0]`)) === 'Boiler 1');

  // Drag the text field with the mouse (it is the selected element, and on top), and read
  // where it ended up from its properties.
  const box = await evaluate(`__t.rect('[data-element-type="text"]')`);
  const from = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: from.x, y: from.y, button: 'left', clickCount: 1 });
  for (let step = 1; step <= 6; step++) {
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: from.x + step * 40, y: from.y + step * 20, button: 'left', buttons: 1 });
  }
  await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: from.x + 240, y: from.y + 120, button: 'left', clickCount: 1 });
  await sleep(300);
  const moved = await evaluate(`__t.rect('[data-element-type="text"]')`);
  const position = await evaluate(`[document.querySelector('[aria-label="X"]').value, document.querySelector('[aria-label="Y"]').value]`);
  check('dragging an element with the mouse moves it', moved.x - box.x > 150 && moved.y - box.y > 70, `${Math.round(box.x)},${Math.round(box.y)} -> ${Math.round(moved.x)},${Math.round(moved.y)}`);
  check('and it lands on the grid', Number(position[0]) % 8 === 0 && Number(position[1]) % 8 === 0, position.join(','));
  check('the screen is marked unsaved', await evaluate(`__t.has('[data-testid="screen-unsaved"]')`));

  await evaluate(`__t.click('Save screen')`); await sleep(900);
  check('after saving it is no longer unsaved', !(await evaluate(`__t.has('[data-testid="screen-unsaved"]')`)));
  const drafts = await (await api(base + '/api/v1/projects/plant_a/hmi/screens?view=draft')).json();
  const savedText = drafts[0]?.canvasData?.components?.find((c) => c.type === 'text');
  check('the controller stored the screen with both elements, at the dragged position',
    drafts[0]?.name === 'Boiler overview' && drafts[0].canvasData.components.length === 2 &&
    String(savedText?.x) === position[0] && String(savedText?.y) === position[1],
    JSON.stringify(savedText));
  check('saving does not release', (await (await api(base + '/api/v1/projects/plant_a/hmi/screens?view=active')).json()).length === 0);

  await evaluate(`__t.click('Release screens')`); await sleep(900);
  check('after releasing, the screen is reported as released', (await evaluate(`__t.text('[data-testid="release-state"]')[0]`)) === 'RELEASED');
  const active = await (await api(base + '/api/v1/projects/plant_a/hmi/screens?view=active')).json();
  check('the released version on the controller is the saved one', JSON.stringify(active[0]?.canvasData) === JSON.stringify(drafts[0]?.canvasData));
  await shot('e2e_5_hmi_screen');

  // 8. The operator runtime: the released screen, full window, with live values.
  await evaluate(`__t.click('Start runtime')`); await sleep(1800);
  check('Start runtime opens the operator runtime', await evaluate(`__t.has('[data-testid="runtime-view"]')`));
  check('the engineering workbench is no longer on screen', !(await evaluate(`__t.has('[data-testid="project-view"]')`)));
  const rtField = await evaluate(`(() => { const el = document.querySelector('[data-testid="runtime-view"] [data-testid="io-field"]'); return el && {
    state: el.getAttribute('data-state'), value: el.querySelector('[data-testid="tag-value"]').textContent }; })()`);
  check('the runtime shows the live value on the released screen', rtField?.state === 'live' && Number(rtField.value) >= 20 && Number(rtField.value) <= 80, JSON.stringify(rtField));
  check('and the released text', (await evaluate(`__t.text('[data-testid="runtime-view"] .textfield')[0]`)) === 'Boiler 1');
  check('it reports the stream as online', (await evaluate(`__t.text('[data-testid="runtime-stream"]')[0]`)) === 'ONLINE');
  check('the address is the runtime, so a reload returns to it', (await evaluate(`location.hash`)) === '#/runtime/plant_a');

  // Another engineer releases a change. The runtime must follow without a reload.
  const revised = structuredClone(drafts[0].canvasData);
  revised.components.find((c) => c.type === 'text').text = 'Boiler 1 — revised';
  await api(`${base}/api/v1/projects/plant_a/hmi/screens/${drafts[0].id}/draft`, {
    method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify(revised) });
  await sleep(700);
  check('a saved but unreleased change does not reach the operator', (await evaluate(`__t.text('[data-testid="runtime-view"] .textfield')[0]`)) === 'Boiler 1');
  await api(`${base}/api/v1/projects/plant_a/hmi/commit`, { method: 'POST' });
  await sleep(1500);
  check('a release made elsewhere appears in the runtime without a reload',
    (await evaluate(`__t.text('[data-testid="runtime-view"] .textfield')[0]`)) === 'Boiler 1 — revised');
  await shot('e2e_6_operator_runtime');

  await evaluate(`__t.click('Exit runtime')`); await sleep(500);
  check('Exit runtime returns to the workbench', await evaluate(`__t.has('[data-testid="project-view"]')`));

  // 9. Go offline: values disappear, on the screen as everywhere else.
  await evaluate(`__t.click('Go offline')`); await sleep(500);
  const offline = await evaluate(`__t.text('[data-testid="tag-value"]')`);
  const fieldOffline = await evaluate(`document.querySelector('[data-testid="io-field"]').getAttribute('data-state')`);
  check('going offline removes the value from the screen', offline.length >= 1 && offline.every((v) => v === '—') && fieldOffline === 'offline', `${offline.join(',')} / ${fieldOffline}`);
  await shot('e2e_7_offline');

  // 10. Settings: the administrator's account and the controller's accounts.
  await evaluate(`__t.click('Project')`); await sleep(150);
  await evaluate(`__t.click('Settings…')`); await sleep(600);
  check('Settings opens on the signed-in account', await evaluate(`__t.has('[data-testid="settings-view"]')`)
    && (await evaluate(`__t.text('[data-testid="my-role"]')`))[0] === 'admin');
  await evaluate(`__t.click('Users')`); await sleep(800);
  const accounts = await evaluate(`__t.text('[data-testid="user-row"] td:first-child')`);
  check('an administrator sees the accounts of the controller', accounts.some((a) => a.startsWith(user)), accounts.join(', '));
  await shot('e2e_8_settings_users');
  await evaluate(`__t.click('Updates')`); await sleep(1500);
  check('Updates says this server runs from no installation, and why',
    (await evaluate(`__t.text('[data-testid="no-installation"]')`)).join(' ').includes('does not run from an installation'));
  check('a release feed that cannot be read is said so, not shown as an empty list',
    (await evaluate(`__t.text('[data-testid="feed-error"]')`)).join(' ').includes('could not be read'));
  await shot('e2e_9_settings_updates');
  await evaluate(`__t.click('Modules')`); await sleep(1200);
  const modules = await evaluate(`__t.text('[data-testid="module-row"]')`);
  check('Settings → Modules lists the Modbus driver module the server runs', modules.some((m) => m.includes('org.mythos.driver.modbus') && m.includes('built in')), modules.join(' | '));
  await shot('e2e_10_settings_modules');
  await evaluate(`__t.click('Sign out')`); await sleep(600);
  check('signing out returns to the sign-in page', await evaluate(`__t.has('[data-testid="sign-in"]')`));
  ws.close();
} catch (e) {
  console.log('FAIL  the script itself failed —', e.message); failures++;
} finally {
  chrome.kill('SIGKILL');
}
console.log(failures === 0 ? '\nALL CHECKS PASSED' : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
