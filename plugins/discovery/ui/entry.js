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
          const rows = said.nodes.map((node) =>
            el(
              'tr',
              { dataset: { testid: 'network-node' } },
              el('td', {}, el('strong', {}, node.name || '(no name)'), el('div', { className: 'mono muted' }, node.id)),
              el('td', {}, node.role || '—'),
              el('td', { className: 'mono' }, node.addresses.map((a) => `${a}:${node.port}`).join(', ') || '—'),
              el('td', { className: 'mono' }, node.version || '—'),
              el('td', {}, el('span', { className: 'settings__badge' }, 'candidate'))
            )
          );
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
            draw(await host.api('/api/v1/m/discovery/nodes'), null);
          } catch (cause) {
            draw(null, `The list of nodes cannot be read: ${cause.message}`);
          }
          if (!stopped) timer = setTimeout(read, EVERY_MS);
        };

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
