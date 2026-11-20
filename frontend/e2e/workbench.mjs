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
  const shot = async (name) => { const m = await send('Page.captureScreenshot', { format: 'png' }); writeFileSync(join(outDir, `${name}.png`), Buffer.from(m.result.data, 'base64'));
    // A control pushed past an edge is invisible to a DOM test: every screenshot measures them all.
    const survey = await evaluate(`(() => { const out = [];
      for (const c of document.querySelectorAll('button, input, select, textarea, a, [role="tab"]')) {
        const r = c.getBoundingClientRect(); if (r.width === 0 || r.height === 0) continue;
        if (getComputedStyle(c).visibility === 'hidden') continue;
        let worst = 0, kind = '';
        const test = (b, k) => { const dx = Math.max(r.right - b.right, b.left - r.left); if (dx > worst) { worst = dx; kind = k; } };
        test({ left: 0, right: innerWidth }, 'viewport');
        for (let p = c.parentElement; p; p = p.parentElement) { const st = getComputedStyle(p);
          if (st.overflowX !== 'visible') test(p.getBoundingClientRect(), ((p.className && String(p.className).split(' ')[0]) || p.tagName) + '/' + st.overflowX); }
        if (worst > 0.5) out.push((c.getAttribute('data-testid') || c.getAttribute('aria-label') || c.textContent.trim().slice(0, 30)) + ' @' + kind + ' ' + Math.round(worst) + 'px');
      } return out; })()`);
    check(`${name}: no control is past the edge of the window or of a box that cuts it off`, survey.length === 0, JSON.stringify(survey));
  };

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

  // 8c. Control logic (ADR-013): a program written in the workbench is checked by the logic
  // module, bound to a tag of the device, built, loaded into the VM runtime, and its result is
  // read back from the tag bus. Three module processes and a simulated device; nothing stubbed.
  check('the logic module adds Programs to the project tree', await evaluate(`__t.has('[data-testid="tree-programs"]')`));
  await evaluate(`__t.clickTestId('tree-add-program')`); await sleep(200);
  await evaluate(`__t.type('Program name', 'Doubler'); __t.press('Program name', 'Enter'); true`); await sleep(1500);
  check('a program added from the tree opens in the program editor', await evaluate(`__t.has('[data-testid="program-editor"]')`));
  check('under a header the module names', (await evaluate(`__t.text('.workarea__path')[0]`)) === 'Boiler HouseProgramsDoubler',
    await evaluate(`__t.text('.workarea__path')[0]`));
  check('a program nothing runs is said to be not loaded', (await evaluate(`__t.text('[data-testid="runtime-mode"]')[0]`)) === 'NOT LOADED',
    await evaluate(`__t.text('[data-testid="runtime-mode"]')[0]`));
  const typeSource = (text) => evaluate(`(() => {
    const area = document.querySelector('[aria-label="Structured Text source"]');
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(area, ${JSON.stringify(text)});
    area.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
  await typeSource('PROGRAM Doubler\nVAR_INPUT seconds : INT; END_VAR\nVAR twice : DINT; ticks : DINT; END_VAR\n  ticks := ticks + 1;\n  twice := seconds * oops;\nEND_PROGRAM\n');
  await evaluate(`__t.click('Check')`); await sleep(1200);
  const diagnostics = await evaluate(`__t.text('[data-testid="diagnostic"]')`);
  check('a mistake is reported by the compiler with its line and column', diagnostics.length === 1 && /^5:\d+oops is not declared$/.test(diagnostics[0]), diagnostics.join(' | '));
  check('and marked in the source', (await evaluate(`__t.text('[data-testid="source-error-mark"]')[0]`)) === 'oops');
  await typeSource('PROGRAM Doubler\nVAR_INPUT seconds : INT; END_VAR\nVAR twice : DINT; ticks : DINT; END_VAR\n  ticks := ticks + 1;\n  twice := seconds * 2;\nEND_PROGRAM\n');
  await evaluate(`__t.click('Check')`); await sleep(1200);
  check('the corrected program compiles', (await evaluate(`__t.text('[data-testid="check-ok"]')[0]`)) === 'COMPILES');
  const variableRows = await evaluate(`__t.text('[data-testid="variable-row"]')`);
  check('and its variables are listed for binding', variableRows.length === 3 && variableRows[0].startsWith('secondsINT') && variableRows[1].startsWith('twiceDINT') && variableRows[2].startsWith('ticksDINT'), variableRows.join(' | '));
  await evaluate(`__t.type('Bind seconds', 'input'); true`); await sleep(200);
  await evaluate(`__t.type('Reads tag for seconds', 'external.BoilerHouse.Unit1.runtime_s'); true`);
  await evaluate(`__t.type('Bind twice', 'published'); true`); await sleep(200);
  await evaluate(`__t.type('Published as for twice', 'internal.BoilerHouse.Doubler.twice'); true`);
  // The program's properties are in the inspector; a Check may have brought Diagnostics forward.
  await evaluate(`__t.click('Properties')`); await sleep(200);
  await evaluate(`__t.type('Interval in milliseconds', '50'); true`);
  await evaluate(`[...document.querySelectorAll('.settings__check')].find((l) => l.textContent.includes('Run when loaded')).querySelector('input').click(); true`);
  check('nothing runs before it is loaded', (await (await api(base + '/api/v1/runtimes')).json()).runtimes.length === 0);
  await evaluate(`__t.clickTestId('program-load')`); await sleep(3000);
  const builtNote = await evaluate(`__t.text('[data-testid="program-built"]')[0]`);
  // Since C6b the note also says how the runtime started: a first load is not changed online.
  check('Load builds the program and loads it into a runtime', /^Version [0-9a-f]{12} was built and loaded\.Not changed online: /.test(builtNote ?? ''), builtNote);
  check('no error is left on screen', !(await evaluate(`__t.has('[data-testid="program-error"]')`)), (await evaluate(`__t.text('[data-testid="program-error"]')`)).join(' '));
  const loadedRuntime = (await (await api(base + '/api/v1/runtimes')).json()).runtimes[0];
  check('the controller runs it in the VM runtime module', loadedRuntime?.id === 'Doubler' && loadedRuntime.module === 'vm' && loadedRuntime.mode === 'run' && loadedRuntime.scans > 0, JSON.stringify(loadedRuntime));
  check('the editor says it runs', (await evaluate(`__t.text('[data-testid="runtime-mode"]')[0]`)) === 'RUN');
  check('at the version that was built', (await evaluate(`__t.text('[data-testid="runtime-version"]')[0]`)) === `Doubler · version ${loadedRuntime?.version}` && builtNote.includes(loadedRuntime?.version));
  // What the program publishes can be put on a screen: the HMI's tag picker offers it (C5).
  const editorTab = (text) => evaluate(`(() => { const t = [...document.querySelectorAll('.editorbar__tab-label')].find((el) => el.textContent.includes(${JSON.stringify(text)})); if (!t) throw new Error('no tab: ' + ${JSON.stringify(text)}); t.click(); return true; })()`);
  await editorTab('Boiler overview'); await sleep(600);
  // Select the I/O field with the mouse, as an engineer does.
  const fieldBox = await evaluate(`__t.rect('[data-element-type="io_field"]')`);
  await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: fieldBox.x + fieldBox.width / 2, y: fieldBox.y + fieldBox.height / 2, button: 'left', clickCount: 1 });
  await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: fieldBox.x + fieldBox.width / 2, y: fieldBox.y + fieldBox.height / 2, button: 'left', clickCount: 1 });
  await sleep(400);
  const offered = await evaluate(`[...document.querySelectorAll('select[aria-label="Tag"] option')].map((o) => o.value)`);
  check('the HMI offers the tag the program publishes, beside the device’s', Array.isArray(offered) && offered.includes('internal.BoilerHouse.Doubler.twice') && offered.includes('external.BoilerHouse.Unit1.steam_temp'), JSON.stringify(offered));
  await editorTab('Programs / Doubler'); await sleep(600);
  const liveBindings = await evaluate(`[...document.querySelectorAll('[data-testid="binding"]')].map((b) => {
    const v = b.querySelector('[data-testid="tag-value"]'); return { kind: b.getAttribute('data-kind'), state: v?.getAttribute('data-state'), value: v?.textContent }; })`);
  const read = liveBindings.find((b) => b.kind === 'input'); const published = liveBindings.find((b) => b.kind === 'published');
  check('the program’s result is live on the tag bus: twice the device’s counter',
    read?.state === 'live' && published?.state === 'live' && Math.abs(Number(published.value) - 2 * Number(read.value)) <= 4 && Number(published.value) > 0,
    JSON.stringify(liveBindings));
  // Watching (ADR-013 C5): every variable's value in the running program, bound to a tag or not.
  const watchedNow = () => evaluate(`[...document.querySelectorAll('[data-testid="variable-row"]')].map((row) => {
    const v = row.querySelector('[data-testid="variable-value"]'); return { name: row.querySelector('td').textContent, state: v?.getAttribute('data-state'), value: v?.textContent }; })`);
  const watchedFirst = await watchedNow(); await sleep(1500); const watchedLater = await watchedNow();
  const ticksOf = (rows) => rows.find((r) => r.name === 'ticks');
  check('a variable bound to no tag shows its value in the running program', ticksOf(watchedFirst)?.state === 'live' && Number(ticksOf(watchedFirst).value) > 0, JSON.stringify(watchedFirst));
  check('and the value follows the program scan by scan', Number(ticksOf(watchedLater)?.value) > Number(ticksOf(watchedFirst)?.value), `${ticksOf(watchedFirst)?.value} -> ${ticksOf(watchedLater)?.value}`);
  const twiceWatched = watchedLater.find((r) => r.name === 'twice'); const secondsWatched = watchedLater.find((r) => r.name === 'seconds');
  check('the values are of one scan: twice is exactly two times seconds', twiceWatched?.state === 'live' && Number(twiceWatched.value) === 2 * Number(secondsWatched?.value), JSON.stringify(watchedLater));
  await shot('e2e_6c_logic');
  // Forcing (ADR-013 C5c): an engineer holds twice at 1234 against the program, which writes
  // seconds * 2 every scan; the editor, the bound tag and the controller all say it is forced.
  await evaluate(`window.prompt = () => '1234'; window.confirm = () => true; document.querySelector('[aria-label="Force twice"]').click(); true`); await sleep(2500);
  const forcedRuntime = (await (await api(base + '/api/v1/runtimes')).json()).runtimes[0];
  check('Force holds the variable: the controller lists it and the tag it is published as',
    forcedRuntime?.forced?.length === 1 && forcedRuntime.forced[0].variable === 'twice' && forcedRuntime.forced[0].value === 1234 && JSON.stringify(forcedRuntime?.forced_tags) === '["internal.BoilerHouse.Doubler.twice"]',
    JSON.stringify(forcedRuntime));
  const forcedRow = await evaluate(`(() => { const row = [...document.querySelectorAll('[data-testid="variable-row"]')].find((r) => r.querySelector('td').textContent === 'twice');
    const v = row.querySelector('[data-testid="variable-value"]'); return { value: v.textContent, forced: v.getAttribute('data-forced') }; })()`);
  check('the editor shows the forced value, marked FORCED', forcedRow.value === '1234FORCED' && forcedRow.forced === 'true', JSON.stringify(forcedRow));
  check('the Runtime panel says what is forced', (await evaluate(`__t.text('[data-testid="runtime-forced-badge"]')[0]`)) === 'FORCED 1'
    && (await evaluate(`__t.text('[data-testid="runtime-forced-variable"]')[0]`)) === 'twice = 1234');
  const forcedTag = await evaluate(`(() => { const b = document.querySelector('[data-testid="binding"][data-kind="published"]');
    return { value: b.querySelector('[data-testid="tag-value"]')?.textContent, marked: !!b.querySelector('[data-testid="tag-forced"]') }; })()`);
  check('the published tag carries the forced value on the tag bus, marked FORCED', forcedTag.value === '1234' && forcedTag.marked, JSON.stringify(forcedTag));
  await shot('e2e_6d_forced');
  await evaluate(`__t.clickTestId('runtime-release-all')`); await sleep(2500);
  const releasedRuntime = (await (await api(base + '/api/v1/runtimes')).json()).runtimes[0];
  const releasedRow = await watchedNow();
  check('Release all hands the variable back to the program', JSON.stringify(releasedRuntime?.forced) === '[]'
    && Number(releasedRow.find((r) => r.name === 'twice')?.value) === 2 * Number(releasedRow.find((r) => r.name === 'seconds')?.value)
    && !(await evaluate(`__t.has('[data-testid="variable-forced"]')`)), JSON.stringify({ releasedRuntime, releasedRow }));
  await evaluate(`__t.clickTestId('runtime-stop')`); await sleep(2500);
  check('Stop puts the program in STOP', (await evaluate(`__t.text('[data-testid="runtime-mode"]')[0]`)) === 'STOP'
    && (await (await api(base + '/api/v1/runtimes')).json()).runtimes[0]?.mode === 'stop');
  const stoppedValue = await evaluate(`(() => { const v = document.querySelector('[data-testid="binding"][data-kind="published"] [data-testid="tag-value"]'); return v.getAttribute('data-state'); })()`);
  check('and its published tag is no longer shown as a current value', stoppedValue !== 'live', stoppedValue);
  const watchedStopped = await watchedNow();
  check('its variables are shown as what it stopped with, not as running values', watchedStopped.length === 3 && watchedStopped.every((r) => r.state === 'stopped'), JSON.stringify(watchedStopped));
  await evaluate(`__t.clickTestId('runtime-unload')`); await sleep(2500);
  check('Unload takes it out of the controller’s runtimes', (await (await api(base + '/api/v1/runtimes')).json()).runtimes.length === 0
    && (await evaluate(`__t.text('[data-testid="runtime-mode"]')[0]`)) === 'NOT LOADED');
  await evaluate(`document.querySelector('.workarea__close').click(); true`); await sleep(400);

  // 8d. A Ladder Diagram (ADR-013 L3b-1): added from the tree as LD, its rungs saved over the
  // API as an agent saves them (the graphical editor is L3b-2), checked with a mistake drawn at
  // its element, bound to the device's counter, loaded, and its contacts drawn live: `odd`
  // follows the counter, so the contact on it changes state as the device counts.
  {
  await evaluate(`__t.clickTestId('tree-add-program')`); await sleep(200);
  await evaluate(`__t.type('Program name', 'Ladder'); __t.type('Program language', 'ld'); __t.press('Program name', 'Enter'); true`); await sleep(1500);
  // Since L3b-2b an engineer gets the editor: a new diagram has its rails and no network yet.
  check('an LD program added from the tree opens in the Ladder Diagram editor, with no network and no text area',
    (await evaluate(`__t.has('[data-testid="ld-editor"]') && __t.has('[data-testid="ld-no-networks"]')`))
      && !(await evaluate(`__t.has('[aria-label="Structured Text source"]')`)),
    await evaluate(`__t.text('[data-testid="ld-panel"]').join(' ')`));
  const languages = await evaluate(`[...document.querySelectorAll('[data-testid="tree-program"]')].map((n) => n.querySelector('.tree__label').textContent + ':' + n.querySelector('[data-testid="tree-program-language"]').textContent)`);
  check('the tree says each program’s language', JSON.stringify(languages) === '["Doubler:ST","Ladder:LD"]', JSON.stringify(languages));
  const rung = (id, x, y, body) => body.replace('@', `localId="${id}"><position x="${x}" y="${y}"/>`);
  const ladder = (contactVariable) => `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="Ladder" pouType="program">
      <interface>
        <inputVars><variable name="seconds"><type><INT/></type></variable></inputVars>
        <localVars>
          <variable name="odd"><type><BOOL/></type></variable>
          <variable name="lamp"><type><BOOL/></type></variable>
          <variable name="dark"><type><BOOL/></type></variable>
        </localVars>
      </interface>
      <body><LD>
        <leftPowerRail localId="1"><position x="0" y="0"/><connectionPointOut formalParameter=""/></leftPowerRail>
        ${rung(10, 40, 20, '<inVariable @<connectionPointOut/><expression>seconds MOD 2 = 1</expression></inVariable>')}
        ${rung(11, 260, 20, '<outVariable @<connectionPointIn><connection refLocalId="10"/></connectionPointIn><expression>odd</expression></outVariable>')}
        ${rung(20, 60, 80, `<contact @<connectionPointIn><connection refLocalId="1"/></connectionPointIn><connectionPointOut/><variable>${contactVariable}</variable></contact>`)}
        ${rung(21, 260, 80, '<coil @<connectionPointIn><connection refLocalId="20"/></connectionPointIn><connectionPointOut/><variable>lamp</variable></coil>')}
        ${rung(30, 60, 140, '<contact negated="true" @<connectionPointIn><connection refLocalId="1"/></connectionPointIn><connectionPointOut/><variable>odd</variable></contact>')}
        ${rung(31, 260, 140, '<coil @<connectionPointIn><connection refLocalId="30"/></connectionPointIn><connectionPointOut/><variable>dark</variable></coil>')}
        <rightPowerRail localId="2"><position x="400" y="0"/><connectionPointIn><connection refLocalId="21"/></connectionPointIn><connectionPointIn><connection refLocalId="31"/></connectionPointIn></rightPowerRail>
      </LD></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
`;
  const saveLadder = async (contactVariable) => (await api(`${base}/api/v1/projects/plant_a/logic/programs/Ladder/source`, {
    method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ source: ladder(contactVariable) }) })).ok;
  check('an agent saves rungs into it over the API', await saveLadder('od'));
  await sleep(1500);
  check('the open diagram follows what was saved elsewhere',
    (await evaluate(`document.querySelectorAll('[data-testid="ld-element"][data-kind="contact"]').length`)) === 2,
    await evaluate(`[...document.querySelectorAll('[data-testid="ld-element"]')].map((e) => e.getAttribute('data-kind')).join()`));
  await evaluate(`__t.click('Check')`); await sleep(1200);
  const ldDiagnostics = await evaluate(`__t.text('[data-testid="diagnostic"]')`);
  check('a mistake in a rung is told at its element', JSON.stringify(ldDiagnostics) === '["Ladder element 20od is not declared"]', JSON.stringify(ldDiagnostics));
  check('and drawn around it in the diagram', (await evaluate(`[...document.querySelectorAll('[data-testid="ld-diagnostic"]')].map((d) => d.getAttribute('data-local-id')).join()`)) === '20');
  check('the corrected rungs are saved', await saveLadder('odd'));
  await sleep(1500);
  await evaluate(`__t.click('Check')`); await sleep(1200);
  check('and compile', (await evaluate(`__t.text('[data-testid="check-ok"]')[0]`)) === 'COMPILES');
  const ldRows = await evaluate(`__t.text('[data-testid="variable-row"] td:first-child')`);
  check('its variables are listed, and none the front-end made for itself', JSON.stringify(ldRows) === '["seconds","odd","lamp","dark"]', JSON.stringify(ldRows));
  check('its rungs are numbered in the order they run, a network each', JSON.stringify(await evaluate(`__t.text('[data-testid="ld-network-select"]')`)) === '["Network 1","Network 2","Network 3"]',
    JSON.stringify(await evaluate(`__t.text('[data-testid="ld-network-select"]')`)));
  await evaluate(`__t.type('Bind seconds', 'input'); true`); await sleep(200);
  await evaluate(`__t.type('Reads tag for seconds', 'external.BoilerHouse.Unit1.runtime_s'); true`);
  // The program's properties are in the inspector; a Check may have brought Diagnostics forward.
  await evaluate(`__t.click('Properties')`); await sleep(200);
  await evaluate(`__t.type('Interval in milliseconds', '50'); true`);
  await evaluate(`[...document.querySelectorAll('.settings__check')].find((l) => l.textContent.includes('Run when loaded')).querySelector('input').click(); true`);
  await evaluate(`__t.clickTestId('program-load')`); await sleep(3000);
  const ldBuilt = await evaluate(`__t.text('[data-testid="program-built"]')[0]`);
  check('Load builds the Ladder Diagram and loads it into a runtime', /^Version [0-9a-f]{12} was built and loaded\.Not changed online: /.test(ldBuilt ?? ''), ldBuilt);
  const ladderRuntime = (await (await api(base + '/api/v1/runtimes')).json()).runtimes.find((r) => r.id === 'Ladder');
  check('the controller runs it in the VM runtime module', ladderRuntime?.mode === 'run' && ladderRuntime.scans > 0, JSON.stringify(ladderRuntime));
  await sleep(1000);
  const live = () => evaluate(`(() => {
    const at = (id) => document.querySelector('[data-testid="ld-element"][data-local-id="' + id + '"]')?.getAttribute('data-state');
    const row = [...document.querySelectorAll('[data-testid="variable-row"]')].find((r) => r.querySelector('td').textContent === 'odd');
    return { live: document.querySelector('[data-testid="ld-live"]')?.getAttribute('data-live'), no: at(20), nc: at(30), lamp: at(21), dark: at(31),
      rail: document.querySelector('[data-testid="ld-link"][data-from="1"][data-to="20"]')?.getAttribute('data-state'),
      odd: row?.querySelector('[data-testid="variable-value"]')?.textContent }; })()`);
  const seen = [];
  for (let i = 0; i < 6; i++) { seen.push(await live()); await sleep(450); }
  check('the diagram is drawn live while its version runs', seen.every((s) => s.live === 'live' && s.rail === 'on'), JSON.stringify(seen));
  check('each contact is drawn from its variable: the negated one the opposite',
    seen.every((s) => (s.no === 'on' || s.no === 'off') && s.nc === (s.no === 'on' ? 'off' : 'on') && s.odd === (s.no === 'on' ? 'TRUE' : 'FALSE')),
    JSON.stringify(seen));
  check('and its state changes with the device’s counter', new Set(seen.map((s) => s.no)).size === 2, JSON.stringify(seen.map((s) => s.no)));
  await shot('e2e_6e_ladder');
  await evaluate(`__t.clickTestId('runtime-stop')`); await sleep(2500);
  check('a stopped Ladder Diagram is drawn with no power', (await live()).live === 'none' && (await live()).no === 'none', JSON.stringify(await live()));
  await evaluate(`__t.clickTestId('runtime-unload')`); await sleep(2500);
  check('Unload takes it out of the controller’s runtimes', !(await (await api(base + '/api/v1/runtimes')).json()).runtimes.some((r) => r.id === 'Ladder'));
  await evaluate(`document.querySelector('.workarea__close').click(); true`); await sleep(400);
  }

  // 8e. Editing a Ladder Diagram (ADR-013 L3b-2b): an engineer builds a seal-in rung from the
  // keyboard (start, stop, the motor coil, the motor contact in a branch around start) and a
  // timer rung from the palette, checks, saves, loads and runs it, and watches the motor coil
  // follow a simulated start and stop button: each forced over the API, as a pushbutton wired
  // to the input would set it.
  {
  await evaluate(`__t.clickTestId('tree-add-program')`); await sleep(200);
  await evaluate(`__t.type('Program name', 'Seal'); __t.type('Program language', 'ld'); __t.press('Program name', 'Enter'); true`); await sleep(1500);
  const sealUrl = `${base}/api/v1/projects/plant_a/logic/programs/Seal`;
  const created = (await (await api(sealUrl)).json()).source;
  check('an engineer gets the Ladder Diagram editor, with no network yet', await evaluate(`__t.has('[data-testid="ld-editor"]') && __t.has('[data-testid="ld-no-networks"]')`));
  // DOM tests cannot see a control pushed off screen: the real browser's rectangles are read.
  const onScreen = (sel) => evaluate(`(() => { const r = __t.rect(${JSON.stringify(sel)});
    return { ...r, inside: r.width > 0 && r.height > 0 && r.x >= 0 && r.y >= 0 && r.x + r.width <= innerWidth + 1 && r.y < innerHeight }; })()`);
  const palette = await onScreen('[data-testid="ld-palette"]');
  const networkList = await onScreen('[data-testid="ld-networks"]');
  const favouritesBar = await onScreen('[data-testid="ld-favourites"]');
  check('the instruction palette, the favourites and the network list are on screen, the palette beside the networks',
    palette.inside && networkList.inside && favouritesBar.inside && palette.width >= 180 && networkList.width >= 300 && palette.x >= networkList.x + networkList.width - 1,
    JSON.stringify({ palette, networkList, favouritesBar }));
  await evaluate(`(() => {
    const keyAt = (key, m = {}) => document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key, shiftKey: !!m.shift, ctrlKey: !!m.ctrl, altKey: !!m.alt, bubbles: true, cancelable: true }));
    window.__ld = {
      focus() { document.querySelector('[data-testid="ld-editor"]').focus(); },
      key: keyAt,
      operand(text, declare) {
        const el = document.activeElement;
        if (!el || el.getAttribute('role') !== 'combobox') throw new Error('no operand field has the focus: ' + (el ? el.outerHTML.slice(0, 80) : 'nothing'));
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, text);
        el.dispatchEvent(new Event('input', { bubbles: true }));
        keyAt('Enter', { ctrl: declare });
      },
      element(kind, operand) {
        return [...document.querySelectorAll('[data-testid="ld-element"][data-kind="' + kind + '"]')].find((e) => (e.querySelector('title')?.textContent ?? '').endsWith(': ' + operand));
      },
      select(kind, operand) {
        const g = window.__ld.element(kind, operand);
        if (!g) throw new Error('no ' + kind + ' ' + operand);
        g.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      },
      palette(id) { document.querySelector('[data-testid="ld-palette-item"][data-item="' + id + '"]').click(); },
    }; return true; })()`);
  // Ctrl+R a network; Shift+F2 start, Shift+F3 stop, Shift+F7 the coil motor, each operand typed
  // where the focus lands and declared with Ctrl+Enter as the editor proposes (inputs for the
  // contacts, an output for the coil).
  await evaluate(`__ld.focus(); __ld.key('r', { ctrl: true }); true`); await sleep(300);
  await evaluate(`__ld.key('F2', { shift: true }); true`); await sleep(300);
  const operandField = await onScreen('[data-testid="ld-operand"]');
  check('a new contact’s operand field has the focus, on screen',
    (await evaluate(`document.activeElement?.getAttribute('data-testid')`)) === 'ld-operand' && operandField.inside, JSON.stringify(operandField));
  await evaluate(`__ld.operand('start', true); true`); await sleep(300);
  await evaluate(`__ld.key('F3', { shift: true }); true`); await sleep(300);
  await evaluate(`__ld.operand('stop', true); true`); await sleep(300);
  await evaluate(`__ld.key('F7', { shift: true }); true`); await sleep(300);
  await evaluate(`__ld.operand('motor', true); true`); await sleep(300);
  // The seal: a branch opened at start (Shift+F8) and closed there (Shift+F9), the motor contact in it.
  // One action a call: what React does with an event shows after it, not within the same script.
  await evaluate(`__ld.select('contact', 'start'); true`); await sleep(300);
  await evaluate(`__ld.focus(); __ld.key('F8', { shift: true }); true`); await sleep(300);
  check('a branch is open at start', await evaluate(`__t.has('[data-testid="ld-branch-open"]')`));
  await evaluate(`__ld.key('F9', { shift: true }); true`); await sleep(300);
  await evaluate(`__ld.key('F2', { shift: true }); true`); await sleep(300);
  await evaluate(`__ld.operand('motor', false); true`); await sleep(300);
  await evaluate(`document.querySelector('[aria-label="Title of network 1"]').focus(); __t.type('Title of network 1', 'Motor seal-in'); true`); await sleep(300);
  await evaluate(`__ld.focus(); true`); await sleep(300);
  // The timer rung from the palette: a network after it, a contact on motor, a TON whose
  // instance is proposed and declared with Enter, its PT, and the coil lamp.
  await evaluate(`__t.clickTestId('ld-add-network'); true`); await sleep(300);
  await evaluate(`__ld.palette('contact-no'); true`); await sleep(300);
  await evaluate(`__ld.operand('motor', false); true`); await sleep(300);
  await evaluate(`__ld.palette('box:TON'); true`); await sleep(300);
  const instance = await evaluate(`[document.activeElement?.getAttribute('data-testid'), document.activeElement?.value]`);
  check('a TON from the palette asks for its instance with a name proposed', JSON.stringify(instance) === '["ld-box-instance","ton1"]', JSON.stringify(instance));
  await evaluate(`__ld.key('Enter'); true`); await sleep(300);
  await evaluate(`(() => { const pt = document.querySelector('[data-testid="ld-pin"][data-formal="PT"] input'); pt.focus(); __ld.operand('T#2s', false); return true; })()`); await sleep(300);
  await evaluate(`__ld.focus(); true`); await sleep(300);
  await evaluate(`__ld.key('F7', { shift: true }); true`); await sleep(300);
  await evaluate(`__ld.operand('lamp', true); true`); await sleep(300);
  const drawn = await evaluate(`[...document.querySelectorAll('[data-testid="ld-network"]')].map((n) =>
    [...n.querySelectorAll('[data-testid="ld-element"]')].filter((e) => !/PowerRail/.test(e.getAttribute('data-kind'))).map((e) => e.querySelector('title').textContent.replace(/ [0-9]+/, '')).join(', '))`);
  check('the two rungs are drawn as built', JSON.stringify(drawn) === JSON.stringify([
    'contact: start, contact: motor, contact: stop, coil: motor', 'contact: motor, block, coil: lamp']), JSON.stringify(drawn));
  check('the program is unsaved until it is saved', await evaluate(`__t.has('[data-testid="program-unsaved"]')`));
  await shot('e2e_6f_ladder_editor');
  await evaluate(`__t.click('Check')`); await sleep(1500);
  check('what the editor built compiles', (await evaluate(`__t.text('[data-testid="check-ok"]')[0]`)) === 'COMPILES',
    JSON.stringify(await evaluate(`__t.text('[data-testid="diagnostic"]')`)));
  const sealRows = await evaluate(`__t.text('[data-testid="variable-row"] td:first-child')`);
  // The timer's instance is listed by its members (ton1.Q, ton1.ET, …).
  check('its variables are the ones declared where they were typed', ['start', 'stop', 'motor', 'lamp', 'ton1.Q', 'ton1.ET'].every((v) => sealRows.includes(v)), JSON.stringify(sealRows));
  await evaluate(`__t.click('Save')`); await sleep(1500);
  const savedSeal = (await (await api(sealUrl)).json()).source;
  const headerOf = (text) => text.slice(0, text.indexOf('<types>')) + text.slice(text.indexOf('</types>'));
  check('Save writes the TC6 document through the source route, its header as the module made it',
    !(await evaluate(`__t.has('[data-testid="program-unsaved"]')`)) && headerOf(savedSeal) === headerOf(created)
      && savedSeal.includes('title="Motor seal-in"') && /<inputVars><variable name="start">/.test(savedSeal),
    savedSeal.slice(0, 200));
  // The program's properties are in the inspector; a Check may have brought Diagnostics forward.
  await evaluate(`__t.click('Properties')`); await sleep(200);
  await evaluate(`__t.type('Interval in milliseconds', '50'); true`);
  await evaluate(`[...document.querySelectorAll('.settings__check')].find((l) => l.textContent.includes('Run when loaded')).querySelector('input').click(); true`);
  await evaluate(`__t.clickTestId('program-load')`); await sleep(3000);
  const sealBuilt = await evaluate(`__t.text('[data-testid="program-built"]')[0]`);
  check('Load builds the edited diagram and loads it', /^Version [0-9a-f]{12} was built and loaded\./.test(sealBuilt ?? ''), sealBuilt);
  const sealRuntime = (await (await api(base + '/api/v1/runtimes')).json()).runtimes.find((r) => r.id === 'Seal');
  check('the controller runs it', sealRuntime?.mode === 'run' && sealRuntime.scans > 0, JSON.stringify(sealRuntime));
  const force = async (variable, value) => (await api(`${base}/api/v1/runtimes/Seal/force`, {
    method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ variable, value }) })).ok;
  const sealLive = () => evaluate(`(() => {
    const at = (kind, operand) => window.__ld.element(kind, operand)?.getAttribute('data-state');
    return { live: [...document.querySelectorAll('[data-testid="ld-live"]')].map((e) => e.getAttribute('data-live')).join(), motor: at('coil', 'motor'), lamp: at('coil', 'lamp') }; })()`);
  await sleep(1000);
  const idle = await sealLive();
  check('drawn live, the motor coil is off before start is pressed', idle.live === 'live' && idle.motor === 'off' && idle.lamp === 'off', JSON.stringify(idle));
  check('start is pressed', await force('start', true)); await sleep(1500);
  const pressed = await sealLive();
  check('the motor coil goes on', pressed.motor === 'on', JSON.stringify(pressed));
  check('start is let go', await force('start', false)); await sleep(1500);
  const sealed = await sealLive();
  check('the motor coil stays on through its own contact', sealed.motor === 'on', JSON.stringify(sealed));
  await sleep(1000);
  const timed = await sealLive();
  check('and the lamp comes on two seconds after the motor', timed.motor === 'on' && timed.lamp === 'on', JSON.stringify(timed));
  check('stop is pressed', await force('stop', true)); await sleep(1500);
  const stopped = await sealLive();
  check('the motor coil goes off, and the lamp with it', stopped.motor === 'off' && stopped.lamp === 'off', JSON.stringify(stopped));
  check('stop is let go', await force('stop', false)); await sleep(1500);
  const after = await sealLive();
  check('the motor coil stays off', after.motor === 'off', JSON.stringify(after));
  await shot('e2e_6g_ladder_editor_live');
  await api(`${base}/api/v1/runtimes/Seal/release`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{}' });
  await evaluate(`__t.clickTestId('runtime-stop')`); await sleep(2500);
  await evaluate(`__t.clickTestId('runtime-unload')`); await sleep(2500);
  check('Unload takes the edited program out of the controller’s runtimes', !(await (await api(base + '/api/v1/runtimes')).json()).runtimes.some((r) => r.id === 'Seal'));
  await evaluate(`document.querySelector('.workarea__close').click(); true`); await sleep(400);
  }

  // 8f. A Function Block Diagram (ADR-013 L3c-1): added from the tree as FBD, its networks saved
  // over the API as an agent saves them, checked with a mistake drawn at its element, bound to
  // the device's counter, loaded, and drawn live: a BOOL line on or off, a number written at its
  // output, a line through a connector, all following the counter. Since L3c-2 an engineer sees
  // it in the FBD editor, which draws each network with the view and its live values.
  {
  await evaluate(`__t.clickTestId('tree-add-program')`); await sleep(200);
  await evaluate(`__t.type('Program name', 'Blender'); __t.type('Program language', 'fbd'); __t.press('Program name', 'Enter'); true`); await sleep(1500);
  check('an FBD program added from the tree opens in the Function Block Diagram editor, with no network, no text area and no LD editor',
    (await evaluate(`__t.has('[data-testid="fbd-editor"]') && __t.has('[data-testid="fbd-no-networks"]') && !__t.has('[data-testid="ld-editor"]')`))
      && !(await evaluate(`__t.has('[aria-label="Structured Text source"]')`)),
    await evaluate(`__t.text('[data-testid="fbd-panel"]').join(' ')`));
  const fbdLanguage = await evaluate(`[...document.querySelectorAll('[data-testid="tree-program"]')].filter((n) => n.querySelector('.tree__label').textContent === 'Blender').map((n) => n.querySelector('[data-testid="tree-program-language"]').textContent)`);
  check('the tree says it is FBD', JSON.stringify(fbdLanguage) === '["FBD"]', JSON.stringify(fbdLanguage));
  const at = (id, x, y, body) => body.replace('@', `localId="${id}"><position x="${x}" y="${y}"/>`);
  const line = (from) => `<connectionPointIn><connection refLocalId="${from}"/></connectionPointIn>`;
  const pin = (formal, inner) => `<variable formalParameter="${formal}">${inner}</variable>`;
  const box = (id, x, y, type, inputs) => at(id, x, y, `<block typeName="${type}" @<inputVariables>${inputs}</inputVariables><inOutVariables/><outputVariables><variable formalParameter="OUT"><connectionPointOut/></variable></outputVariables></block>`);
  const literal = (text) => `<connectionPointIn><expression>${text}</expression></connectionPointIn>`;
  const diagram = (secondsVariable) => `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="Blender" pouType="program">
      <interface>
        <inputVars><variable name="seconds"><type><INT/></type></variable></inputVars>
        <localVars>
          <variable name="odd"><type><BOOL/></type></variable>
          <variable name="even"><type><BOOL/></type></variable>
          <variable name="tenfold"><type><INT/></type></variable>
        </localVars>
      </interface>
      <body><FBD>
        ${at(10, 0, 20, `<inVariable @<connectionPointOut/><expression>${secondsVariable}</expression></inVariable>`)}
        ${box(11, 120, 20, 'MOD', pin('IN1', line(10)) + pin('IN2', literal('2')))}
        ${box(12, 240, 20, 'EQ', pin('IN1', line(11)) + pin('IN2', literal('1')))}
        ${at(13, 360, 20, `<outVariable @${line(12)}<expression>odd</expression></outVariable>`)}
        ${at(20, 0, 120, '<inVariable @<connectionPointOut/><expression>seconds</expression></inVariable>')}
        ${box(21, 120, 120, 'MUL', pin('IN1', line(20)) + pin('IN2', literal('10')))}
        ${at(22, 240, 120, `<connector name="TEN" @${line(21)}</connector>`)}
        ${at(23, 0, 180, '<continuation name="TEN" @<connectionPointOut/></continuation>')}
        ${at(24, 120, 180, `<outVariable @${line(23)}<expression>tenfold</expression></outVariable>`)}
        ${at(30, 0, 260, '<inVariable @<connectionPointOut/><expression>odd</expression></inVariable>')}
        ${box(31, 120, 260, 'NOT', pin('IN', line(30)))}
        ${at(32, 240, 260, `<outVariable @${line(31)}<expression>even</expression></outVariable>`)}
      </FBD></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
`;
  const saveDiagram = async (secondsVariable) => (await api(`${base}/api/v1/projects/plant_a/logic/programs/Blender/source`, {
    method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ source: diagram(secondsVariable) }) })).ok;
  check('an agent saves networks into it over the API', await saveDiagram('second'));
  await sleep(1500);
  check('the open diagram follows what was saved elsewhere',
    (await evaluate(`document.querySelectorAll('[data-testid="fbd-element"][data-kind="block"]').length`)) === 4,
    await evaluate(`[...document.querySelectorAll('[data-testid="fbd-element"]')].map((e) => e.getAttribute('data-kind')).join()`));
  await evaluate(`__t.click('Check')`); await sleep(1200);
  const fbdDiagnostics = await evaluate(`__t.text('[data-testid="diagnostic"]')`);
  check('a mistake in a network is told at its element', JSON.stringify(fbdDiagnostics) === '["Blender element 10second is not declared"]', JSON.stringify(fbdDiagnostics));
  check('and drawn around it in the diagram', (await evaluate(`[...document.querySelectorAll('[data-testid="fbd-diagnostic"]')].map((d) => d.getAttribute('data-local-id')).join()`)) === '10');
  check('the corrected networks are saved', await saveDiagram('seconds'));
  await sleep(1500);
  await evaluate(`__t.click('Check')`); await sleep(1200);
  check('and compile', (await evaluate(`__t.text('[data-testid="check-ok"]')[0]`)) === 'COMPILES');
  const fbdRows = await evaluate(`__t.text('[data-testid="variable-row"] td:first-child')`);
  check('its variables are listed, and none the front-end made for itself', JSON.stringify(fbdRows) === '["seconds","odd","even","tenfold"]', JSON.stringify(fbdRows));
  const fbdNetworks = await evaluate(`[...document.querySelectorAll('[data-testid="fbd-network-edit"]')].map((n) => n.getAttribute('data-network') + ':' + n.querySelector('[data-testid="fbd-network"]')?.getAttribute('data-ids'))`);
  check('its networks are numbered in the order they run, the connector joining its line', JSON.stringify(fbdNetworks) === '["1:10,11,12,13","2:20,21,24","3:30,31,32"]', JSON.stringify(fbdNetworks));
  check('the network with the connector is kept as the agent wrote it, and says why',
    JSON.stringify(await evaluate(`[...document.querySelectorAll('[data-testid="fbd-network-edit"]')].map((n) => n.getAttribute('data-kept') ?? '')`)) === '["","true",""]'
      && (await evaluate(`__t.text('[data-testid="fbd-network-kept"]')[0]`)).includes('connector TEN'));
  await evaluate(`__t.type('Bind seconds', 'input'); true`); await sleep(200);
  await evaluate(`__t.type('Reads tag for seconds', 'external.BoilerHouse.Unit1.runtime_s'); true`);
  // The program's properties are in the inspector; a Check may have brought Diagnostics forward.
  await evaluate(`__t.click('Properties')`); await sleep(200);
  await evaluate(`__t.type('Interval in milliseconds', '50'); true`);
  await evaluate(`[...document.querySelectorAll('.settings__check')].find((l) => l.textContent.includes('Run when loaded')).querySelector('input').click(); true`);
  await evaluate(`__t.clickTestId('program-load')`); await sleep(3000);
  const fbdBuilt = await evaluate(`__t.text('[data-testid="program-built"]')[0]`);
  check('Load builds the Function Block Diagram and loads it into a runtime', /^Version [0-9a-f]{12} was built and loaded\./.test(fbdBuilt ?? ''), fbdBuilt);
  const fbdRuntime = (await (await api(base + '/api/v1/runtimes')).json()).runtimes.find((r) => r.id === 'Blender');
  check('the controller runs it in the VM runtime module', fbdRuntime?.mode === 'run' && fbdRuntime.scans > 0, JSON.stringify(fbdRuntime));
  await sleep(1000);
  const fbdLive = () => evaluate(`(() => {
    const value = (from, output) => document.querySelector('[data-testid="fbd-value"][data-from="' + from + '"][data-output="' + output + '"]');
    const lineState = (from, to) => document.querySelector('[data-testid="fbd-line"][data-from="' + from + '"][data-to="' + to + '"]')?.getAttribute('data-state');
    return { live: document.querySelector('[data-testid="fbd-live"]')?.getAttribute('data-live'),
      seconds: value(20, '')?.textContent, tenfold: value(21, 'OUT')?.textContent, tenfoldState: value(21, 'OUT')?.getAttribute('data-state'),
      continued: value(23, '')?.textContent, mod: value(11, 'OUT')?.textContent,
      odd: lineState(12, 13), even: lineState(31, 32), through: lineState(23, 24) }; })()`);
  const fbdSeen = [];
  for (let i = 0; i < 6; i++) { fbdSeen.push(await fbdLive()); await sleep(450); }
  check('the diagram is drawn live while its version runs', fbdSeen.every((s) => s.live === 'live'), JSON.stringify(fbdSeen));
  check('a number is written at its output: MUL gives ten times what its line carries, and the continuation the same',
    fbdSeen.every((s) => s.tenfoldState === 'value' && Number(s.tenfold) === 10 * Number(s.seconds) && s.continued === s.tenfold && s.through === 'value'),
    JSON.stringify(fbdSeen));
  check('a BOOL line is drawn on or off: odd and its negation opposite, as MOD says',
    fbdSeen.every((s) => (s.odd === 'on' || s.odd === 'off') && s.even === (s.odd === 'on' ? 'off' : 'on') && s.mod === (s.odd === 'on' ? '1' : '0')),
    JSON.stringify(fbdSeen));
  check('and they change with the device’s counter', new Set(fbdSeen.map((s) => s.odd)).size === 2 && new Set(fbdSeen.map((s) => s.tenfold)).size > 1,
    JSON.stringify(fbdSeen.map((s) => `${s.odd}/${s.tenfold}`)));
  await shot('e2e_6h_fbd');
  await evaluate(`__t.clickTestId('runtime-stop')`); await sleep(2500);
  const fbdStopped = await fbdLive();
  check('a stopped Function Block Diagram is drawn with no values', fbdStopped.live === 'none' && fbdStopped.tenfold === undefined, JSON.stringify(fbdStopped));
  await evaluate(`__t.clickTestId('runtime-unload')`); await sleep(2500);
  check('Unload takes it out of the controller’s runtimes', !(await (await api(base + '/api/v1/runtimes')).json()).runtimes.some((r) => r.id === 'Blender'));
  await evaluate(`document.querySelector('.workarea__close').click(); true`); await sleep(400);
  }

  // 8g. Editing a Function Block Diagram (ADR-013 L3c-2): an engineer builds a seal-in from the
  // keyboard (an AND of start and a negated stop, assigned to motor; an OR joined into its first
  // input for the motor's own seal) and a timer network from the Instructions task card, checks,
  // saves, loads and runs it, and watches the motor's assignment follow a start and a stop forced
  // as a pushbutton would set them.
  {
  await evaluate(`__t.clickTestId('tree-add-program')`); await sleep(200);
  await evaluate(`__t.type('Program name', 'Mix'); __t.type('Program language', 'fbd'); __t.press('Program name', 'Enter'); true`); await sleep(1500);
  const mixUrl = `${base}/api/v1/projects/plant_a/logic/programs/Mix`;
  const mixCreated = (await (await api(mixUrl)).json()).source;
  check('an engineer gets the FBD editor, with no network yet', await evaluate(`__t.has('[data-testid="fbd-editor"]') && __t.has('[data-testid="fbd-no-networks"]')`));
  const onScreen = (sel) => evaluate(`(() => { const r = __t.rect(${JSON.stringify(sel)});
    return { ...r, inside: r.width > 0 && r.height > 0 && r.x >= 0 && r.y >= 0 && r.x + r.width <= innerWidth + 1 && r.y < innerHeight }; })()`);
  const fbdPalette = await onScreen('[data-testid="fbd-palette"]');
  const fbdNetworkList = await onScreen('[data-testid="fbd-networks"]');
  const fbdFavourites = await onScreen('[data-testid="fbd-favourites"]');
  check('the FBD instructions, its favourites and its network list are on screen, the instructions beside the networks',
    fbdPalette.inside && fbdNetworkList.inside && fbdFavourites.inside && fbdPalette.width >= 180 && fbdNetworkList.width >= 300 && fbdPalette.x >= fbdNetworkList.x + fbdNetworkList.width - 1,
    JSON.stringify({ fbdPalette, fbdNetworkList, fbdFavourites }));
  await evaluate(`(() => {
    const keyAt = (key, m = {}) => document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key, shiftKey: !!m.shift, ctrlKey: !!m.ctrl, altKey: !!m.alt, bubbles: true, cancelable: true }));
    window.__fbd = {
      focus() { document.querySelector('[data-testid="fbd-editor"]').focus(); },
      key: keyAt,
      operand(text, declare) {
        const el = document.activeElement;
        if (!el || el.getAttribute('role') !== 'combobox') throw new Error('no operand field has the focus: ' + (el ? el.outerHTML.slice(0, 80) : 'nothing'));
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, text);
        el.dispatchEvent(new Event('input', { bubbles: true }));
        keyAt('Enter', { ctrl: declare });
      },
      pin(network, formal) {
        const t = document.querySelectorAll('[data-testid="fbd-network-edit"]')[network - 1].querySelector('[data-testid="fbd-pin-target"][data-formal="' + formal + '"]');
        if (!t) throw new Error('no input ' + formal + ' in network ' + network);
        t.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      },
      element(kind, operand) {
        return [...document.querySelectorAll('[data-testid="fbd-element"][data-kind="' + kind + '"]')].find((e) => (e.querySelector('title')?.textContent ?? '').endsWith(': ' + operand));
      },
      palette(id) { document.querySelector('[data-testid="fbd-palette-item"][data-item="' + id + '"]').click(); },
    }; return true; })()`);
  // Ctrl+R a network; Shift+F2 an AND, its IN1 field taking the focus: start, declared with
  // Ctrl+Enter as proposed (a BOOL input).
  await evaluate(`__fbd.focus(); __fbd.key('r', { ctrl: true }); true`); await sleep(300);
  await evaluate(`__fbd.key('F2', { shift: true }); true`); await sleep(300);
  const fbdOperand = await onScreen('[data-testid="fbd-pin-operand"]');
  check('a new AND’s first input takes the focus, on screen',
    (await evaluate(`document.activeElement?.getAttribute('data-testid')`)) === 'fbd-pin-operand' && fbdOperand.inside, JSON.stringify(fbdOperand));
  await evaluate(`__fbd.operand('start', true); true`); await sleep(300);
  // IN2: its point clicked, Enter to its field, stop; negated.
  await evaluate(`__fbd.pin(1, 'IN2'); true`); await sleep(300);
  await evaluate(`__fbd.focus(); __fbd.key('Enter'); true`); await sleep(300);
  await evaluate(`__fbd.operand('stop', true); true`); await sleep(300);
  await evaluate(`document.querySelector('[data-testid="fbd-pin"][data-formal="IN2"] [data-testid="fbd-pin-negated"]').click(); true`); await sleep(300);
  // Shift+F7: the assignment of the AND's output, motor, declared a BOOL output.
  await evaluate(`__fbd.focus(); __fbd.key('F7', { shift: true }); true`); await sleep(300);
  await evaluate(`__fbd.operand('motor', true); true`); await sleep(300);
  // The seal: IN1 selected, Shift+F3 puts an OR there, start into its IN1; motor into its IN2.
  await evaluate(`__fbd.pin(1, 'IN1'); true`); await sleep(300);
  await evaluate(`__fbd.focus(); __fbd.key('F3', { shift: true }); true`); await sleep(300);
  await evaluate(`__fbd.operand('motor', false); true`); await sleep(300);
  await evaluate(`document.querySelector('[aria-label="Title of network 1"]').focus(); __t.type('Title of network 1', 'Motor seal-in'); true`); await sleep(300);
  await evaluate(`__fbd.focus(); true`); await sleep(300);
  // The timer from the Instructions task card: its instance proposed and declared with Enter,
  // IN motor, PT 2 s, Q assigned to lamp.
  await evaluate(`__t.clickTestId('fbd-add-network'); true`); await sleep(300);
  await evaluate(`__fbd.palette('box:TON'); true`); await sleep(300);
  const fbdInstance = await evaluate(`[document.activeElement?.getAttribute('data-testid'), document.activeElement?.value]`);
  check('a TON from the card asks for its instance with a name proposed', JSON.stringify(fbdInstance) === '["fbd-box-instance","ton1"]', JSON.stringify(fbdInstance));
  await evaluate(`__fbd.key('Enter'); true`); await sleep(300);
  for (const [formal, text] of [['IN', 'motor'], ['PT', 'T#2s']]) {
    await evaluate(`(() => { const f = document.querySelector('[data-testid="fbd-pin"][data-formal="${formal}"] input[role="combobox"]'); f.focus(); __fbd.operand(${JSON.stringify(text)}, false); return true; })()`); await sleep(300);
  }
  await evaluate(`__fbd.focus(); __fbd.key('F7', { shift: true }); true`); await sleep(300);
  await evaluate(`__fbd.operand('lamp', true); true`); await sleep(300);
  const fbdDrawn = await evaluate(`[...document.querySelectorAll('[data-testid="fbd-network-edit"]')].map((n) =>
    [...n.querySelectorAll('[data-testid="fbd-element"]')].map((e) => e.getAttribute('data-kind') === 'block' ? e.querySelector('text').textContent : e.querySelector('title').textContent.replace(/ [0-9]+/, '')).sort().join(', '))`);
  check('the two networks are drawn as built', JSON.stringify(fbdDrawn) === JSON.stringify([
    'AND, OR, inVariable: motor, inVariable: start, inVariable: stop, outVariable: motor', 'TON, inVariable: T#2s, inVariable: motor, outVariable: lamp']), JSON.stringify(fbdDrawn));
  check('the program is unsaved until it is saved', await evaluate(`__t.has('[data-testid="program-unsaved"]')`));
  await shot('e2e_6i_fbd_editor');
  await evaluate(`__t.click('Check')`); await sleep(1500);
  check('what the FBD editor built compiles', (await evaluate(`__t.text('[data-testid="check-ok"]')[0]`)) === 'COMPILES',
    JSON.stringify(await evaluate(`__t.text('[data-testid="diagnostic"]')`)));
  const mixRows = await evaluate(`__t.text('[data-testid="variable-row"] td:first-child')`);
  check('its variables are the ones declared where they were typed', ['start', 'stop', 'motor', 'lamp', 'ton1.Q'].every((v) => mixRows.includes(v)), JSON.stringify(mixRows));
  await evaluate(`__t.click('Save')`); await sleep(1500);
  const savedMix = (await (await api(mixUrl)).json()).source;
  const mixHeader = (text) => text.slice(0, text.indexOf('<types>')) + text.slice(text.indexOf('</types>'));
  check('Save writes the TC6 document through the source route, its header as the module made it',
    !(await evaluate(`__t.has('[data-testid="program-unsaved"]')`)) && mixHeader(savedMix) === mixHeader(mixCreated) && savedMix.includes('title="Motor seal-in"'),
    savedMix.slice(0, 200));
  await evaluate(`__t.click('Properties')`); await sleep(200);
  await evaluate(`__t.type('Interval in milliseconds', '50'); true`);
  await evaluate(`[...document.querySelectorAll('.settings__check')].find((l) => l.textContent.includes('Run when loaded')).querySelector('input').click(); true`);
  await evaluate(`__t.clickTestId('program-load')`); await sleep(3000);
  const mixBuilt = await evaluate(`__t.text('[data-testid="program-built"]')[0]`);
  check('Load builds the edited diagram and loads it', /^Version [0-9a-f]{12} was built and loaded\./.test(mixBuilt ?? ''), mixBuilt);
  const mixForce = async (variable, value) => (await api(`${base}/api/v1/runtimes/Mix/force`, {
    method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ variable, value }) })).ok;
  const mixLive = () => evaluate(`(() => {
    const into = (operand) => { const e = window.__fbd.element('outVariable', operand); const id = e?.getAttribute('data-local-id');
      return document.querySelector('[data-testid="fbd-line"][data-to="' + id + '"]')?.getAttribute('data-state'); };
    return { live: document.querySelector('[data-testid="fbd-live"]')?.getAttribute('data-live'), motor: into('motor'), lamp: into('lamp') }; })()`);
  await sleep(1000);
  const mixIdle = await mixLive();
  check('drawn live, the line into motor is off before start is pressed', mixIdle.live === 'live' && mixIdle.motor === 'off' && mixIdle.lamp === 'off', JSON.stringify(mixIdle));
  check('start is pressed', await mixForce('start', true)); await sleep(1500);
  check('the line into motor goes on', (await mixLive()).motor === 'on', JSON.stringify(await mixLive()));
  check('start is let go', await mixForce('start', false)); await sleep(1500);
  check('motor stays on through its own seal', (await mixLive()).motor === 'on', JSON.stringify(await mixLive()));
  await sleep(1000);
  const mixTimed = await mixLive();
  check('and the lamp comes on two seconds after the motor', mixTimed.motor === 'on' && mixTimed.lamp === 'on', JSON.stringify(mixTimed));
  await shot('e2e_6j_fbd_editor_live');
  check('stop is pressed', await mixForce('stop', true)); await sleep(1500);
  const mixStopped = await mixLive();
  check('motor goes off, and the lamp with it', mixStopped.motor === 'off' && mixStopped.lamp === 'off', JSON.stringify(mixStopped));
  await api(`${base}/api/v1/runtimes/Mix/release`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{}' });
  await evaluate(`__t.clickTestId('runtime-stop')`); await sleep(2500);
  await evaluate(`__t.clickTestId('runtime-unload')`); await sleep(2500);
  check('Unload takes the edited FBD program out of the controller’s runtimes', !(await (await api(base + '/api/v1/runtimes')).json()).runtimes.some((r) => r.id === 'Mix'));
  await evaluate(`document.querySelector('.workarea__close').click(); true`); await sleep(400);
  }

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
