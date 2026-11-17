// End-to-end check of the workbench, run by scripts/e2e.sh.
//
// Drives headless Chromium over the DevTools protocol against a real mythos-server and a real
// (simulated) Modbus TCP device: real elements are clicked and typed into, and what is asserted
// is what the page shows and what the controller reports. Nothing is stubbed.
//
// usage: node workbench.mjs <screenshot dir> <server base URL> <simulator port> [chromium]
import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
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
// The control program e2e.sh compiled (frontend/e2e/fan.st): its name, bytecode and symbols.
if (!process.env.MYTHOS_E2E_PROGRAM) {
  console.error('workbench.mjs: MYTHOS_E2E_PROGRAM is required (the compiled control program)');
  process.exit(2);
}
const fanProgram = JSON.parse(readFileSync(process.env.MYTHOS_E2E_PROGRAM, 'utf8'));
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

  // 8a. The plant (ADR-012 §2.4): this project is one device, this controller, on this node,
  // which runs it: the comparison says so.
  await evaluate(`__t.dblclick('tree-plant')`); await sleep(1500);
  const plantRows = await evaluate(`__t.text('[data-testid="plant-device"] strong')`);
  check('the plant is this controller until another device is added', JSON.stringify(plantRows) === '["This controller"]', JSON.stringify(plantRows));
  check('and it runs what the project says', (await evaluate(`__t.text('[data-testid="plant-device-state"]')[0]`)) === 'SAME AS PROJECT',
    await evaluate(`__t.text('[data-testid="plant-device-state"]')[0]`));
  // ADR-012 §2.6: this node is the only device, so it reads no other device's tags, and says so.
  const linkCell = await evaluate(`document.querySelector('[data-testid="plant-device"]').children[3].textContent`);
  check('no other device is read here, and that is what it says', linkCell === 'not read here', linkCell);
  const remote = await (await api(base + '/api/v1/remote')).json();
  check('the node follows no other device', Array.isArray(remote.devices) && remote.devices.length === 0, JSON.stringify(remote));
  await evaluate(`document.querySelector('.workarea__close').click(); true`); await sleep(400);

  // 8b. A module's workbench UI (ADR-011 §2.3): the notes service module brings a tree section
  // and an editor of its own; the workbench has no code for either.
  check('the notes module adds its section to the project tree', await evaluate(`__t.has('[data-testid="tree-notes"]')`));
  check('and lists the note it holds', JSON.stringify(await evaluate(`__t.text('[data-testid="tree-note"]')`)) === '["shift-log"]',
    JSON.stringify(await evaluate(`__t.text('[data-testid="tree-note"]')`)));
  await evaluate(`__t.dblclick('tree-note')`); await sleep(800);
  check('its editor opens in the work area with the note’s text',
    (await evaluate(`document.querySelector('[data-testid="note-text"]')?.value`)) === 'Boiler 1 inspected');
  check('under a header the module names', (await evaluate(`__t.text('.workarea__path')[0]`)) === 'Boiler HouseNotesshift-log',
    await evaluate(`__t.text('.workarea__path')[0]`));
  await evaluate(`(() => { const t = document.querySelector('[data-testid="note-text"]'); t.value = 'Boiler 1 inspected, valve 3 replaced'; true })()`);
  await evaluate(`__t.click('Save note')`); await sleep(800);
  const saved = await (await api(`${base}/api/v1/projects/plant_a/notes/notes/shift-log`)).json();
  check('saving goes through the module to its own data', saved.text === 'Boiler 1 inspected, valve 3 replaced', JSON.stringify(saved));
  // Another engineer writes a note: the module's event reaches the tree without a reload.
  await api(`${base}/api/v1/projects/plant_a/notes/notes/handover`, {
    method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ text: 'nights' }) });
  await sleep(1500);
  check('a note written elsewhere appears in the tree without a reload',
    JSON.stringify(await evaluate(`__t.text('[data-testid="tree-note"]')`)) === '["handover","shift-log"]',
    JSON.stringify(await evaluate(`__t.text('[data-testid="tree-note"]')`)));
  await shot('e2e_6b_module_ui');
  await evaluate(`document.querySelector('.workarea__close').click(); true`); await sleep(400);
  check('closing the module’s editor returns to the screen', await evaluate(`__t.has('[data-testid="io-field"]')`));

  // 9. Go offline: values disappear, on the screen as everywhere else.
  await evaluate(`__t.click('Go offline')`); await sleep(500);
  const offline = await evaluate(`__t.text('[data-testid="tag-value"]')`);
  const fieldOffline = await evaluate(`document.querySelector('[data-testid="io-field"]').getAttribute('data-state')`);
  check('going offline removes the value from the screen', offline.length >= 1 && offline.every((v) => v === '—') && fieldOffline === 'offline', `${offline.join(',')} / ${fieldOffline}`);
  await shot('e2e_7_offline');

  // 9b. The Portal's "MythOS on this network" (the discovery module, ADR-012 §2.2): this edge
  // announces itself, listens, and lists no node it has not heard.
  await evaluate(`__t.click('Project')`); await sleep(150);
  await evaluate(`__t.click('Open or create a project')`); await sleep(500);
  await evaluate(`__t.clickTestId('portal-module-network')`); await sleep(2500);
  const network = (await evaluate(`__t.text('[data-testid="network-status"]')`))[0] ?? '';
  check('the Portal lists MythOS on this network, as the discovery module hears it',
    network.includes('This node announces itself.') && network.includes('Listening for others.'), network);
  check('a node heard is offered as a candidate, never as a connection',
    (await evaluate(`__t.text('[data-testid="network-node"] .settings__badge')`)).every((b) => b === 'candidate'));
  await shot('e2e_9b_network');
  await evaluate(`__t.click('Project view')`); await sleep(800);
  check('and the project is one click away', await evaluate(`__t.has('[data-testid="project-view"]')`));

  // 10. Settings: the administrator's account and the controller's accounts.
  await evaluate(`__t.click('Project')`); await sleep(150);
  await evaluate(`__t.click('Settings…')`); await sleep(600);
  check('Settings opens on the signed-in account', await evaluate(`__t.has('[data-testid="settings-view"]')`)
    && (await evaluate(`__t.text('[data-testid="my-role"]')`))[0] === 'admin');
  // The node this server is (ADR-012 §2.1): started without a role, it is an edge, listening
  // on every interface of the port e2e.sh gave it.
  await evaluate(`__t.click('Node')`); await sleep(800);
  const nodeId = (await evaluate(`__t.text('[data-testid="node-id"]')`))[0] ?? '';
  check('Settings → Node shows this node’s id', /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(nodeId), nodeId);
  check('and that it is an edge, answering plain HTTP on this computer only',
    (await evaluate(`document.querySelector('input[value="edge"]')?.checked`)) === true &&
    (await evaluate(`__t.text('[data-testid="node-listening"]')[0]`)) === `127.0.0.1:${new URL(base).port}`,
    await evaluate(`__t.text('[data-testid="node-listening"]')[0]`));
  const nodeHttps = (await evaluate(`__t.text('[data-testid="node-https"]')[0]`)) ?? '';
  const nodeCert = (await evaluate(`__t.text('[data-testid="node-fingerprint"]')[0]`)) ?? '';
  check('and HTTPS on every interface, with its certificate’s fingerprint to compare',
    /^0\.0\.0\.0:\d+$/.test(nodeHttps) && /^([0-9a-f]{4} ){15}[0-9a-f]{4}$/.test(nodeCert), `${nodeHttps} ${nodeCert}`);
  const apiNode = await (await api(`${base}/api/v1/node`)).json();
  check('the API says the same', apiNode.id === nodeId && apiNode.role === 'edge' &&
    apiNode.https.fingerprint === nodeCert.replaceAll(' ', ''), JSON.stringify(apiNode));
  // Pairing (ADR-012 §2.3): no node paired yet; an administrator makes a one-use code.
  await evaluate(`__t.click('Pairing')`); await sleep(800);
  check('Settings → Pairing says this node is paired with no other', await evaluate(`__t.has('[data-testid="pairings-empty"]')`));
  await evaluate(`__t.click('Make a pairing code')`); await sleep(600);
  const pairingCode = (await evaluate(`__t.text('[data-testid="pairing-code"] code')[0]`)) ?? '';
  check('and makes a pairing code to read to the node that pairs', /^[A-Z2-9]{4}-[A-Z2-9]{4}$/.test(pairingCode), pairingCode);
  await evaluate(`__t.click('Fleet')`); await sleep(800);
  check('Settings → Fleet says this node is paired with no other', await evaluate(`__t.has('[data-testid="fleet-empty"]')`));
  // 10a. Runtimes (ADR-013 §2.5): nothing is loaded; a program is loaded over the API, shown on
  // the page as the controller reports it, stopped and run again from the page, and unloaded.
  {
  await evaluate(`__t.click('Runtimes')`); await sleep(1500);
  const noRuntime = (await evaluate(`__t.text('[data-testid="runtimes-empty"]')`))[0] ?? '';
  check('Settings → Runtimes says no runtime is loaded, and which module could run one',
    noRuntime.startsWith('No runtime is loaded. A program can be loaded into: vm'), noRuntime);
  const runtimeModules = await evaluate(`[...document.querySelectorAll('[data-testid="runtime-module"]')].map((r) => ({
    id: r.querySelector('[data-testid="runtime-module-id"]').textContent,
    profile: [...r.querySelector('[data-testid="runtime-module-profile"]').children].map((c) => c.textContent) }))`);
  check('and lists the VM runtime module with the profile it declares',
    runtimeModules.length === 1 && runtimeModules[0].id === 'org.mythos.runtime.vm' && runtimeModules[0].profile.includes('name pc'),
    JSON.stringify(runtimeModules));

  // The simulated device's writable register (10) becomes a tag, and the program commands it.
  const SECONDS = 'external.BoilerHouse.Unit1.runtime_s';
  const FAN = 'external.BoilerHouse.Unit1.fan_speed';
  const SAFE_SPEED = 7;
  const put = (path, body) => api(base + path, { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
  const ioNow = (await (await api(base + '/api/v1/projects/plant_a/io')).json()).config;
  ioNow.connections[0].config.mappings.push({ ...ioNow.connections[0].config.mappings[0], uri: FAN, address: 10 });
  const ioPut = await put('/api/v1/projects/plant_a/io', ioNow);
  check('the device’s writable register is added as a tag', ioPut.ok, `${ioPut.status} ${(await ioPut.text()).slice(0, 200)}`);
  const fan = { id: 'fan', module: 'vm', run: true, program: {
    name: fanProgram.name, version: 'e2e-1', interval_ms: 50, code: fanProgram.code, symbols: fanProgram.symbols,
    inputs: [{ variable: 'seconds', tag: SECONDS }, { variable: 'seconds_ok', tag: SECONDS, of: 'good' }],
    outputs: [{ variable: 'speed', tag: FAN, safe: SAFE_SPEED }],
    // A runtime is a connection, and a connection has tags: the program publishes its setpoint.
    published: [{ variable: 'setpoint', tag: 'internal.BoilerHouse.Fan.setpoint', writable: true }] } };
  const load = await put('/api/v1/projects/plant_a/runtimes', { instances: [fan] });
  const loaded = await load.json();
  check('the controller loads the program into the VM runtime and applies it',
    load.ok && loaded.applied === true && JSON.stringify(loaded.changes?.started) === '["runtime:fan"]', JSON.stringify(loaded).slice(0, 300));

  const runtimeRow = () => evaluate(`(() => {
    const r = document.querySelector('[data-testid="runtime-row"]');
    if (!r) return null;
    const t = (id) => r.querySelector('[data-testid="' + id + '"]')?.textContent.trim() ?? null;
    return { id: t('runtime-id'), module: t('runtime-module-of'), program: t('runtime-program'), version: t('runtime-version'),
      mode: t('runtime-mode'), modeClass: r.querySelector('[data-testid="runtime-mode"]')?.className ?? null,
      scans: t('runtime-scans'), lastScan: t('runtime-last-scan'), tag: t('runtime-output-tag'), safe: t('runtime-output-safe'),
      written: t('runtime-output-written'), outputsSafe: t('runtime-outputs-safe'), stale: r.getAttribute('data-stale'),
      staleFlag: t('runtime-stale') };
  })()`);
  const runtimeApi = async () => (await (await api(base + '/api/v1/runtimes')).json()).runtimes[0];
  // The page reads the list once a second: what it shows must become what is waited for.
  const untilRow = async (what) => {
    let row = null;
    for (let i = 0; i < 40; i++) { row = await runtimeRow(); if (row && what(row)) break; await sleep(250); }
    return row;
  };

  const running = await untilRow((r) => r.mode === 'run' && r.written === '300');
  check('the page shows the runtime without a reload: its module, program, version, RUN and what its output was last written',
    running?.id === 'fan' && running.module === 'vm' && running.program === 'Fan' && running.version === 'e2e-1' &&
    running.mode === 'run' && running.modeClass.includes('badge--good') && running.tag === FAN &&
    running.safe === String(SAFE_SPEED) && running.written === '300' && running.outputsSafe === 'written by the program' &&
    running.stale === 'false', JSON.stringify(running));
  const apiRunning = await runtimeApi();
  check('the controller says the same', apiRunning?.mode === 'run' && apiRunning.outputs[0].written === 300 && apiRunning.outputs_safe === false,
    JSON.stringify(apiRunning));
  await sleep(1600);
  const later = await runtimeRow();
  check('its scan figures on the page are measured and follow the program',
    /^\d+$/.test(running?.scans ?? '') && Number(later?.scans) > Number(running.scans) && /^\d+ µs$/.test(later.lastScan ?? ''),
    `${running?.scans} -> ${later?.scans}, last ${later?.lastScan}`);
  // A command that is off the screen is a command an engineer does not have.
  const stopButton = await evaluate(`__t.rect('button[aria-label="Stop fan"]')`);
  const pageWidth = await evaluate(`window.innerWidth`);
  check('Run and Stop are on the screen, not beyond its edge',
    stopButton.width > 0 && stopButton.x >= 0 && stopButton.x + stopButton.width <= pageWidth, `${JSON.stringify(stopButton)} in ${pageWidth}`);
  await shot('e2e_8a_runtime_running');

  // The controller stops answering (the browser is taken off the network): the page must say
  // so and stop presenting what it last read as what the runtime does now.
  await send('Network.enable');
  const network = (offline) => send('Network.emulateNetworkConditions', { offline, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
  await network(true);
  const lastKnown = await untilRow((r) => r.stale === 'true');
  const readError = (await evaluate(`__t.text('[data-testid="runtimes-read-error"]')`))[0] ?? '';
  check('when the controller does not answer, the page says so and marks the runtime as the last known state, with its age and without the colour of RUN',
    lastKnown?.stale === 'true' && /^LAST KNOWN · \d+ s ago$/.test(lastKnown.staleFlag ?? '') && lastKnown.mode === 'run' &&
    !lastKnown.modeClass.includes('badge--good') && readError.startsWith('The runtimes could not be read:'),
    `${JSON.stringify(lastKnown)} ${readError}`);
  await shot('e2e_8a2_runtime_last_known');
  await network(false);
  const current = await untilRow((r) => r.stale === 'false');
  check('and when it answers again the mark is gone',
    current?.stale === 'false' && current.staleFlag === null && current.modeClass.includes('badge--good') &&
    !(await evaluate(`__t.has('[data-testid="runtimes-read-error"]') || __t.has('[data-testid="runtimes-stale"]')`)), JSON.stringify(current));

  await evaluate(`document.querySelector('button[aria-label="Stop fan"]').click(); true`);
  const stopped = await untilRow((r) => r.mode === 'stop' && r.outputsSafe === 'AT SAFE VALUES');
  check('Stop on the page puts the program in STOP, and its output is confirmed at its safe value',
    stopped?.mode === 'stop' && !stopped.modeClass.includes('badge--good') && stopped.outputsSafe === 'AT SAFE VALUES' &&
    stopped.written === String(SAFE_SPEED), JSON.stringify(stopped));
  const commandAnswer = (await evaluate(`__t.text('[data-testid="runtime-command-answer"]')`))[0] ?? '';
  check('the page shows the controller’s answer to the command', commandAnswer.startsWith('The controller answered for fan: asked stop'), commandAnswer);
  const apiStopped = await runtimeApi();
  check('the controller says the same', apiStopped?.mode === 'stop' && apiStopped.outputs_safe === true && apiStopped.outputs[0].written === SAFE_SPEED,
    JSON.stringify(apiStopped));
  await shot('e2e_8b_runtime_stopped');

  // Run asks first (a cold start). The dialog is answered here; what it asked is checked.
  await evaluate(`window.__asked = []; window.confirm = (message) => { window.__asked.push(message); return true; };
    document.querySelector('button[aria-label="Run fan"]').click(); true`);
  const again = await untilRow((r) => r.mode === 'run' && r.written === '300');
  const askedRun = await evaluate(`window.__asked`);
  check('Run on the page asks first, then the program runs again and writes its output',
    askedRun.length === 1 && askedRun[0].includes('Put fan in RUN?') && again?.mode === 'run' && again.written === '300',
    `${JSON.stringify(askedRun)} ${JSON.stringify(again)}`);

  const unload = await put('/api/v1/projects/plant_a/runtimes', { instances: [] });
  check('the program is unloaded over the API', unload.ok, String(unload.status));
  let gone = false;
  for (let i = 0; i < 40 && !gone; i++) {
    gone = await evaluate(`!__t.has('[data-testid="runtime-row"]') && __t.has('[data-testid="runtimes-empty"]')`);
    if (!gone) await sleep(250);
  }
  check('and leaves the page, which again says no runtime is loaded', gone);
  }

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
  check('and the notes service module', modules.some((m) => m.includes('org.mythos.test.notes')), modules.join(' | '));
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
