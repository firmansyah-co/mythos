// The workbench UI of the notes test module (ADR-011 §2.3), written against
// `@mythos/workbench-sdk` in plain JavaScript and DOM: a module brings whatever it draws with.
// `scripts/e2e.sh` serves it beside the module and `frontend/e2e/workbench.mjs` uses it.

const el = (tag, { dataset = {}, style = '', ...props } = {}, ...children) => {
  // `dataset` and `style` are read-only properties of an element: they are filled, not assigned.
  const node = Object.assign(document.createElement(tag), props);
  for (const [key, value] of Object.entries(dataset)) node.dataset[key] = value;
  if (style) node.style.cssText = style;
  node.append(...children);
  return node;
};

export default {
  id: 'org.mythos.test.notes',

  activate(host) {
    const base = (project) => `/api/v1/projects/${encodeURIComponent(project.id)}/notes`;

    host.register.treeSection({
      id: 'notes',
      mount(container, { project }) {
        let open = true;
        let notes = null;
        let error = null;

        const draw = () => {
          const header = el(
            'div',
            {
              className: 'tree__node',
              role: 'treeitem',
              tabIndex: 0,
              dataset: { testid: 'tree-notes' },
              style: 'padding-left: 22px',
              onclick: () => {
                open = !open;
                draw();
              },
            },
            el('span', { className: 'tree__twisty' }, open ? '▾' : '▸'),
            el('span', { className: 'tree__label' }, 'Notes')
          );
          const rows = !open
            ? []
            : error
              ? [el('p', { className: 'tree__note', dataset: { testid: 'tree-notes-unreadable' } }, error)]
              : notes === null
                ? [el('p', { className: 'tree__note' }, 'Reading the notes…')]
                : notes.map((name) =>
                    el(
                      'div',
                      {
                        className: 'tree__node',
                        role: 'treeitem',
                        tabIndex: 0,
                        dataset: { testid: 'tree-note' },
                        style: 'padding-left: 38px',
                        ondblclick: () => host.openEditor('note', name),
                        onkeydown: (event) => event.key === 'Enter' && host.openEditor('note', name),
                      },
                      el('span', { className: 'tree__twisty' }),
                      el('span', { className: 'tree__label' }, name)
                    )
                  );
          container.replaceChildren(header, ...rows);
        };

        const read = () =>
          host
            .api(`${base(project)}/notes`)
            .then((list) => {
              notes = list;
              error = null;
            })
            .catch((cause) => {
              notes = null;
              error = `The notes could not be read: ${cause.message}`;
            })
            .finally(draw);

        draw();
        void read();
        // Another workbench (or MCP) wrote a note: the list follows without a reload.
        return host.events.subscribe('notes_changed', (data) => {
          if (data && data.project === project.id) void read();
        });
      },
    });

    host.register.editor({
      kind: 'note',
      title: (note) => ['Notes', note ?? ''],
      mount(container, { project, target }) {
        const text = el('textarea', {
          className: 'input',
          rows: 6,
          disabled: true,
          dataset: { testid: 'note-text' },
          ariaLabel: 'Note text',
        });
        const status = el('p', { className: 'muted', dataset: { testid: 'note-status' } }, 'Reading…');
        const save = el('button', { type: 'button', className: 'btn', disabled: true }, 'Save note');
        const url = `${base(project)}/notes/${encodeURIComponent(target ?? '')}`;

        save.onclick = () => {
          save.disabled = true;
          host
            .api(url, {
              method: 'PUT',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ text: text.value }),
            })
            .then(() => {
              status.textContent = 'Saved.';
              host.log.info(`Note “${target}” saved.`);
            })
            .catch((cause) => {
              status.textContent = `Not saved: ${cause.message}`;
            })
            .finally(() => {
              save.disabled = false;
            });
        };

        host
          .api(url)
          .then((note) => {
            text.value = note.text;
            text.disabled = false;
            save.disabled = false;
            status.textContent = '';
          })
          .catch((cause) => {
            status.textContent = `The note could not be read: ${cause.message}`;
          });

        container.replaceChildren(
          el('div', { className: 'editor__column', dataset: { testid: 'note-editor' } }, text, save, status)
        );
      },
    });
  },
};
