// The discovery module's workbench UI (ADR-012 §2.2): "MythOS on this network" in the Portal.
// Plain JavaScript and DOM against @mythos/workbench-sdk. What it lists are candidates: a node
// heard is never trusted or connected to because it was heard (ADR-009); pairing (step 4) is
// what makes one trusted.

const el = (tag, { dataset = {}, ...props } = {}, ...children) => {
  const node = Object.assign(document.createElement(tag), props);
  for (const [key, value] of Object.entries(dataset)) node.dataset[key] = value;
  node.append(...children);
  return node;
};

/** How often the list is read again while it is shown. */
const EVERY_MS = 5000;

export default {
  id: 'org.mythos.discovery',

  activate(host) {
    return host.register.portalSection({
      id: 'network',
      label: 'MythOS on this network',
      mount(container) {
        let stopped = false;
        let timer = null;
        // The row a pairing form is open on, and what it says.
        let pairing = null;
        let pairedIds = new Set();

        const spoken = (fp) => (fp || '').match(/.{1,4}/g)?.join(' ') ?? '';
        // Where to reach a node: an IPv4 address it was heard at, other than loopback, at the port
        // it says it answers HTTPS on.
        const reachAt = (node) => {
          const ip = node.addresses.find((a) => /^\d+\.\d+\.\d+\.\d+$/.test(a) && !a.startsWith('127.'));
          return ip && node.https_port ? `${ip}:${node.https_port}` : null;
        };

        const pairForm = (node) => {
          const address = reachAt(node);
          const code = el('input', { 'aria-label': 'Pairing code', placeholder: 'code made on that node' });
          code.setAttribute('aria-label', 'Pairing code');
          const said = el('div', { dataset: { testid: 'network-pair-result' } }, pairing.message || '');
          const pair = el('button', { type: 'button', className: 'btn btn--primary' }, 'Pair');
          pair.onclick = async () => {
            pair.disabled = true;
            try {
              const done = await host.api('/api/v1/pairings', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ address, fingerprint: node.fingerprint, code: code.value, grant: 'viewer' }),
              });
              pairing = null;
              host.log.info(`Paired with ${done.peer.name}.`);
              void read();
            } catch (cause) {
              pairing.message = cause.message;
              said.textContent = cause.message;
              pair.disabled = false;
            }
          };
          return el(
            'tr',
            { dataset: { testid: 'network-pair-form' } },
            el(
              'td',
              { colSpan: 5 },
              el(
                'p',
                { className: 'muted' },
                `Compare this fingerprint with the one ${node.name} shows in its Settings → Node, then give the pairing code made there:`
              ),
              el('p', { className: 'mono' }, spoken(node.fingerprint)),
              el('div', { className: 'settings__inline' }, code, pair),
              said
            )
          );
        };

        const draw = (said, error) => {
          const heading = el('h2', {}, 'MythOS on this network');
          const intro = el(
            'p',
            { className: 'muted' },
            'Nodes heard on this network. A node heard is a candidate, not a trusted controller: ' +
              'nothing connects to it or deploys to it until it is paired.'
          );
          if (error) {
            container.replaceChildren(
              heading,
              intro,
              el('p', { className: 'form__error', role: 'alert', dataset: { testid: 'network-error' } }, error)
            );
            return;
          }
          if (!said) {
            container.replaceChildren(heading, intro, el('p', { className: 'muted' }, 'Listening…'));
            return;
          }
          const status = el(
            'p',
            { className: 'muted', dataset: { testid: 'network-status' } },
            said.error
              ? `Discovery cannot work here: ${said.error}`
              : `${said.announcing ? 'This node announces itself.' : 'This node does not announce itself.'} ` +
                  `${said.listening ? 'Listening for others.' : 'Not listening.'}`
          );
          const rows = said.nodes.flatMap((node) => {
            const paired = pairedIds.has(node.id);
            const address = reachAt(node);
            let action;
            if (paired) {
              action = el('span', { className: 'settings__badge settings__badge--current' }, 'paired');
            } else if (!address || !node.fingerprint) {
              action = el('span', { className: 'settings__badge', title: 'It announces no HTTPS address and certificate' }, 'candidate');
            } else {
              const open = el('button', { type: 'button', className: 'btn btn--flat', dataset: { testid: 'network-pair' } }, 'Pair…');
              open.onclick = () => {
                pairing = { id: node.id, message: '' };
                draw(said, null);
              };
              action = el('span', {}, el('span', { className: 'settings__badge' }, 'candidate'), ' ', open);
            }
            const row = el(
              'tr',
              { dataset: { testid: 'network-node' } },
              el('td', {}, el('strong', {}, node.name || '(no name)'), el('div', { className: 'mono muted' }, node.id)),
              el('td', {}, node.role || '—'),
              el('td', { className: 'mono' }, address || node.addresses.slice(0, 3).join(', ') || '—'),
              el('td', { className: 'mono' }, node.version || '—'),
              el('td', {}, action)
            );
            return pairing && pairing.id === node.id && !paired ? [row, pairForm(node)] : [row];
          });
          const table = rows.length
            ? el(
                'table',
                { className: 'grid' },
                el(
                  'thead',
                  {},
                  el('tr', {}, ...['Node', 'Role', 'Address', 'Version', ''].map((h) => el('th', {}, h)))
                ),
                el('tbody', {}, ...rows)
              )
            : el('p', { className: 'muted', dataset: { testid: 'network-empty' } }, 'No other MythOS node heard yet.');
          container.replaceChildren(heading, intro, status, table);
        };

        const read = async () => {
          try {
            const heard = await host.api('/api/v1/m/discovery/nodes');
            try {
              pairedIds = new Set((await host.api('/api/v1/pairings')).peers.map((p) => p.id));
            } catch {
              pairedIds = new Set();
            }
            draw(heard, null);
          } catch (cause) {
            draw(null, `The list of nodes cannot be read: ${cause.message}`);
          }
          if (!stopped) timer = setTimeout(tick, EVERY_MS);
        };

        // Read again every few seconds, but not while a pairing form is open: a redraw would
        // lose the code being typed.
        function tick() {
          if (stopped) return;
          if (pairing) {
            timer = setTimeout(tick, EVERY_MS);
            return;
          }
          void read();
        }

        draw(null, null);
        void read();
        return () => {
          stopped = true;
          if (timer) clearTimeout(timer);
        };
      },
    });
  },
};
