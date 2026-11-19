# @mythos/workbench-sdk

The contract between the MythOS workbench and a module's workbench UI (ADR-011 §2.3). It is
types only, plus `defineModule` to check a default export against them.

A module ships `ui/entry.js` in its package (or `modules-ui/<id>/entry.js` beside the directory
of its executable). The server serves it at `/modules/<id>/ui/entry.js` while the module is
enabled; the workbench imports it after sign-in and calls `activate(host)` once.

```js
export default {
  id: 'org.example.notes', // the module's manifest id
  activate(host) {
    host.register.treeSection({
      id: 'notes',
      mount(container, { project }) {
        container.textContent = `Notes of ${project.name}`;
        container.ondblclick = () => host.openEditor('note', 'shift-log');
        return () => container.replaceChildren();
      },
    });
    host.register.editor({
      kind: 'note',
      title: (note) => ['Notes', note],
      mount(container, { project, target }) {
        host.api(`/api/v1/projects/${project.id}/notes/notes/${target}`)
          .then((note) => (container.textContent = note.text));
      },
    });
    return host.events.subscribe('notes_changed', () => host.titlesChanged());
  },
};
```

- **Mounts.** Each contribution is `mount(container, context)`, returning what takes it down.
  Draw with anything; bundle what you use (React included). Use the workbench's CSS classes
  (`tree__node`, `btn`, `card`, …) to look like it.
- **Data.** `host.api` calls the controller as the signed-in user; `host.telemetry` gives the
  live values the workbench already holds (never a value while the stream is closed);
  `host.events.subscribe(name)` hears events a service module sends.
- **Names.** Editor kinds, view ids and tree-section ids are global; registering one that is
  taken throws.

A working example in plain JavaScript is the notes test module's UI,
`crates/mythos-module-tests/ui/entry.js`, which the e2e scenario uses.
