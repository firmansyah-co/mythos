import { useEffect, useRef, useState } from 'react';
import { api } from './api/client';
import { startControllerPolling, stopControllerPolling, useController } from './data/controller';
import { useIoConfig } from './data/ioConfig';
import { formatLocation, parseLocation } from './data/location';
import { log } from './data/log';
import { useSession } from './data/session';
import { goOffline, goOnline, useTelemetry } from './data/telemetry';
import { CORE_EDITOR_KINDS, useWorkbench } from './data/workbench';
import { loadModuleUis, unloadModuleUis } from './modules/loader';
import { ModuleViewHost } from './modules/ModuleViewHost';
import { AboutDialog } from './shell/AboutDialog';
import { PortalView } from './shell/PortalView';
import { ProjectView } from './shell/ProjectView';
import { SettingsView } from './shell/SettingsView';
import { SignIn } from './shell/SignIn';

/**
 * The workbench, once the controller knows who is using it. Until then only the sign-in page
 * is shown, and nothing of the controller is asked for.
 */
export function App() {
  const phase = useSession((state) => state.phase);

  useEffect(() => {
    void useSession.getState().refresh();
  }, []);

  // Signed out (by hand or because the session ended): nothing of the controller stays open.
  useEffect(
    () =>
      useSession.subscribe((state, previous) => {
        if (state.phase !== 'signed-out' || previous.phase !== 'signed-in') return;
        goOffline();
        useWorkbench.getState().closeProject();
      }),
    []
  );

  // The modules' workbench UIs are loaded once someone may use the workbench, and taken down
  // when they sign out: what a module shows is read with the signed-in user's rights.
  const signedIn = phase === 'signed-in' || phase === 'open';
  useEffect(() => {
    if (!signedIn) return;
    void loadModuleUis();
    return unloadModuleUis;
  }, [signedIn]);

  return signedIn ? <Workbench /> : <SignIn />;
}

function Workbench() {
  const view = useWorkbench((state) => state.view);
  const projectId = useWorkbench((state) => state.project?.id ?? null);
  const activeProjectId = useController((state) => state.activeProjectId);
  const [about, setAbout] = useState(false);

  // Return to the project and editor named in the URL, once, when the page loads.
  useEffect(() => {
    const target = parseLocation(window.location.hash);
    if (!target) return;
    let cancelled = false;
    api
      .getProject(target.projectId)
      .then((project) => {
        if (cancelled) return;
        const workbench = useWorkbench.getState();
        workbench.openProject(project);
        if (target.editor) workbench.openEditor(target.editor.kind, target.editor.connectionId);
        if (target.view) workbench.showModuleView(target.view.id, target.view.target);
        log.info(`Project “${project.name}” opened.`);
      })
      .catch((cause: unknown) => {
        if (cancelled) return;
        log.error(
          `The project in the address could not be opened: ${
            cause instanceof Error ? cause.message : String(cause)
          }`
        );
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Keep the URL describing what is on screen.
  useEffect(
    () =>
      useWorkbench.subscribe((state) => {
        const editor = state.editors.find((candidate) => candidate.id === state.activeEditorId);
        const next = formatLocation(
          state.project && state.view === 'module' && state.moduleView
            ? { projectId: state.project.id, view: state.moduleView }
            : state.project
            ? {
                projectId: state.project.id,
                editor:
                  editor &&
                  editor.kind !== 'connection' &&
                  (CORE_EDITOR_KINDS as readonly string[]).includes(editor.kind)
                    ? { kind: editor.kind as 'overview', connectionId: editor.target }
                    : undefined,
              }
            : null
        );
        if (window.location.hash !== next) window.history.replaceState(null, '', next);
      }),
    []
  );

  // The open project's I/O configuration is read when the project opens.
  useEffect(() => {
    if (projectId === null) {
      useIoConfig.getState().reset();
      return;
    }
    void useIoConfig.getState().load(projectId);
  }, [projectId]);

  // The controller's own state is read for as long as a project is open.
  useEffect(() => {
    if (projectId === null) return;
    startControllerPolling();
    return stopControllerPolling;
  }, [projectId]);

  // Go online once when a project that the controller runs is opened. A project the controller
  // does not run has no live values of its own, so the workbench stays offline for it. After
  // that the engineer decides: going offline by hand is not undone by this effect.
  const wentOnlineFor = useRef<string | null>(null);
  useEffect(() => {
    if (projectId === null) {
      wentOnlineFor.current = null;
      return;
    }
    if (activeProjectId === undefined) return;
    if (activeProjectId === projectId) {
      if (wentOnlineFor.current !== projectId) {
        wentOnlineFor.current = projectId;
        goOnline();
      }
    } else if (useTelemetry.getState().wantOnline) {
      goOffline();
    }
  }, [projectId, activeProjectId]);

  // The session log records every change in the link to the controller.
  useEffect(
    () =>
      useTelemetry.subscribe((state, previous) => {
        if (state.stream === previous.stream) return;
        if (state.stream === 'OPEN') log.info('Online: the telemetry stream is open.');
        else if (state.stream === 'CLOSED') log.error('The telemetry stream was lost. Reconnecting.');
        else if (state.stream === 'IDLE') log.info('Offline.');
      }),
    []
  );

  useEffect(
    () =>
      useController.subscribe((state, previous) => {
        if (state.error && state.error !== previous.error) {
          log.error(`The controller cannot be reached: ${state.error}`);
        } else if (!state.error && previous.error) {
          log.info('The controller is answering again.');
        }
      }),
    []
  );

  return (
    <>
      {view === 'portal' ? (
        <PortalView />
      ) : view === 'settings' ? (
        <SettingsView />
      ) : view === 'module' ? (
        <ModuleViewHost />
      ) : (
        <ProjectView onAbout={() => setAbout(true)} />
      )}
      {about && <AboutDialog onClose={() => setAbout(false)} />}
    </>
  );
}
