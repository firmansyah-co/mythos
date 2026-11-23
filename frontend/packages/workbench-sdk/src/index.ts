// The contract between the MythOS workbench and the workbench UI of a module (ADR-011 §2.3).
//
// A module's UI is an ES module, `entry.js`, served by the controller and imported by the
// workbench after sign-in. Its default export is a `WorkbenchModule`: the workbench calls
// `activate(host)` once, and the module registers what it contributes through `host.register`.
//
// Every contribution is a `mount`: the workbench gives it an element to draw into and gets back
// a function that takes it down. A module brings whatever it draws with (React, plain DOM, …);
// nothing of the workbench's own code is shared at run time, so a module built against this
// contract keeps working when the workbench is rebuilt. The workbench's CSS is shared: using its
// class names (`tree__row`, `btn`, `grid`, …) makes a module look like the rest of it.

/** Stop something: an unsubscribe, an unregister, an unmount. */
export type Dispose = () => void;

/** Draw into `container`; return what takes it down again. */
export type Mount<Context> = (container: HTMLElement, context: Context) => Dispose | void;

export interface Project {
  id: string;
  name: string;
}

import type { StreamState, TagSample } from './quality';
export type { StreamState, TagSample, TagView, QualityTone } from './quality';

export interface TelemetrySnapshot {
  tags: ReadonlyMap<string, TagSample>;
  stream: StreamState;
  lastBatchAtMs: number | null;
}

/** One connection of the running project, as the controller reports it. */
export interface ConnectionStatus {
  id: string;
  driver: string;
  /** `RUNNING`, `CONFIG_ERROR` or `STOPPED`. */
  state: string;
  /** The URIs of the tags it provides. */
  tags: string[];
}

/** The running project's field I/O. */
export interface IoStatus {
  connections: ConnectionStatus[];
}

/** What the workbench knows of the controller, as it last polled it. */
export interface ControllerView {
  /** The project the controller runs; `null` when none, `undefined` before it has answered. */
  activeProjectId: string | null | undefined;
  /** The running project's connections; `null` until known. */
  io: IoStatus | null;
  /** Why the controller cannot be reached, when it cannot. */
  error: string | null;
  /**
   * What a binding may name, whether or not the project runs: the tag URIs the open project's
   * I/O configuration provides, as the engineer is editing it, and the `internal.` tags its
   * control programs publish, as its runtimes configuration says.
   */
  configuredTags: string[];
  /**
   * Every tag bound to a variable an engineer forces (ADR-013 §2.5), as the controller's
   * runtimes last said: an input the program no longer takes, an output written the forced
   * value, a published tag giving it. A tag here shows a value that is neither the program's
   * nor the field's, and is marked so wherever it is shown. `null` when not known (the
   * runtimes could not be read): nothing is then marked, and nothing is claimed unforced.
   * A runtime whose module does not answer contributes none, because the controller knows
   * none of its forces.
   */
  forcedTags: string[] | null;
}

export interface Identity {
  username: string;
  display_name: string;
  role: 'viewer' | 'operator' | 'engineer' | 'admin';
}

export interface EditorContext {
  project: Project;
  /** What the editor was opened on (a screen id, …), if anything. */
  target?: string;
}

export interface TreeContext {
  project: Project;
}

export interface ViewContext {
  project: Project;
  /** What the view was shown with (a screen id, …), if anything. */
  target?: string;
}

/** A section of the project tree. */
export interface TreeSection {
  id: string;
  /** Sections are drawn in this order after the workbench's own; lower first. */
  order?: number;
  mount: Mount<TreeContext>;
}

/** An editor in the work area, opened with `host.openEditor(kind, target)`. */
export interface Editor {
  kind: string;
  /**
   * Where the editor is, after the project's name: one segment (`"Pumps"`) or several
   * (`["HMI screens", "Pumps"]`). Asked again on every render and after `host.titlesChanged()`.
   */
  title: (target?: string) => string | string[];
  /** Whether it shows live values, which makes its header say ONLINE while the stream is open. */
  live?: boolean;
  mount: Mount<EditorContext>;
}

/** A page of the Portal, the start page shown before a project is open. */
export interface PortalSection {
  id: string;
  /** Its entry in the Portal's navigation. */
  label: string;
  mount: Mount<Record<string, never>>;
}

/** A full-window view (an operator runtime), shown with `host.showView(id, target)`. */
export interface View {
  /** Also the first word of its address: `#/<id>/<project>[/<target>]`. */
  id: string;
  /** When set, the project toolbar has a button with this label that shows the view. */
  launch?: string;
  mount: Mount<ViewContext>;
}

/** A panel for the inspector or the task cards, set by an editor while it wants it shown. */
export interface Panel {
  title: string;
  mount: Mount<{ project: Project }>;
}

/** What a module says is wrong with the work open in it, for the inspector's Diagnostics tab. */
export interface DiagnosticsPanel extends Panel {
  /**
   * How many problems it shows, added to the count on the tab. `null` when it does not know
   * (nothing was checked yet): nothing is added, and nothing is claimed to be fine.
   */
  count: number | null;
}

/**
 * What `host.api` throws when the controller refused a request or could not be reached: an
 * `Error` whose message says what was answered, with the answer itself beside it.
 */
export interface ApiRefusal extends Error {
  /** The HTTP status of the answer; `null` when there was no answer (the controller was not reached). */
  status: number | null;
  /** The answer's body as JSON (`{ error, … }` from a module); `null` when it was not JSON, or there was none. */
  body: unknown;
}

export interface WorkbenchHost {
  /** The version of this contract the workbench speaks. */
  readonly sdk: 1;

  /** The open project, if any, and changes to it. */
  project(): Project | null;
  onProject(listener: (project: Project | null) => void): Dispose;

  /** Who is signed in, if the controller has accounts. */
  user(): Identity | null;

  /**
   * A request to the controller, as the signed-in user. Resolves with the JSON body; throws an
   * `Error` whose message says what the controller answered, and ends the session on a 401.
   * What it throws is an {@link ApiRefusal}: the answer's status and its body, so a module UI can
   * show why its own module refused (the diagnostics of a refused save, …).
   */
  api<T = unknown>(path: string, init?: RequestInit): Promise<T>;

  /** Live tag values from the workbench's one telemetry stream. */
  telemetry: {
    get(uri: string): TagSample | undefined;
    /** Called after every batch, and when the stream opens or closes. */
    subscribe(listener: () => void): Dispose;
    /** Whether the stream is open now: a value is only live while it is. */
    online(): boolean;
    /** The stream's state, for saying why there is no value. */
    stream(): StreamState;
    /**
     * Everything at once: the last batch by tag URI, the stream's state, and the controller's
     * time of the last batch. The same object until one of them changes.
     */
    snapshot(): TelemetrySnapshot;
    /** Open the stream (go online), as the engineer's Go online does. */
    goOnline(): void;
  };

  /** The controller's state as the workbench polls it, and when it changes. */
  controller: {
    get(): ControllerView;
    subscribe(listener: () => void): Dispose;
  };

  /** Named events the controller sends on the stream (a module's own included). */
  events: {
    subscribe(name: string, listener: (data: unknown) => void): Dispose;
  };

  /** The session log at the bottom of the workbench. */
  log: { info(message: string): void; error(message: string): void };

  openEditor(kind: string, target?: string): void;
  /** Close the editor of `kind` on `target`, if it is open (the thing it edits was deleted). */
  closeEditor(kind: string, target?: string): void;
  /** The module's editor titles changed (something was renamed); the workbench redraws them. */
  titlesChanged(): void;
  showView(id: string, target?: string): void;
  exitView(): void;

  /**
   * Show `panel` in the inspector's Properties tab (or take it away, with `null`) on behalf of
   * `owner`. Setting a panel selects it: selecting anything else in the workbench (a tag, a
   * connection) hides it until it is set again.
   */
  setInspector(owner: string, panel: Panel | null): void;
  /**
   * Say whether `owner` holds changes not yet saved; closing the project asks first while any
   * owner does.
   */
  setUnsaved(owner: string, unsaved: boolean): void;

  /** Show `panel` above the workbench's own task cards (or take it away, with `null`). */
  setTaskCard(owner: string, panel: Panel | null): void;

  /**
   * Show `panel` in the inspector's Diagnostics tab, above the controller's I/O problems (or
   * take it away, with `null`) on behalf of `owner`. Set again with the same `mount` and another
   * `count`, only the count on the tab changes: what is drawn is not drawn again.
   */
  setDiagnostics(owner: string, panel: DiagnosticsPanel | null): void;
  /** Bring the Diagnostics tab forward, opening the inspector if it is collapsed, as a compile does. */
  showDiagnostics(): void;

  register: {
    treeSection(section: TreeSection): Dispose;
    editor(editor: Editor): Dispose;
    view(view: View): Dispose;
    portalSection(section: PortalSection): Dispose;
  };
}

/** What a module's `entry.js` exports by default. */
export interface WorkbenchModule {
  /** The module's manifest id. */
  id: string;
  /** Called once, after sign-in. May return what undoes it. */
  activate(host: WorkbenchHost): Dispose | void;
}

/** Type-checks a module's default export. */
export function defineModule(module: WorkbenchModule): WorkbenchModule {
  return module;
}
