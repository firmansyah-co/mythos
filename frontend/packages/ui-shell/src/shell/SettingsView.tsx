import { ArrowLeft, LogOut } from 'lucide-react';
import { hasRole } from '../api/client';
import { useSession } from '../data/session';
import { useWorkbench } from '../data/workbench';
import { SETTINGS_SECTIONS } from './settings/sections';

/** The settings: one section at a time, chosen on the left. */
export function SettingsView() {
  const user = useSession((state) => state.user);
  const signOut = useSession((state) => state.signOut);
  const current = useWorkbench((state) => state.settingsSection);
  const showSettings = useWorkbench((state) => state.showSettings);
  const closeSettings = useWorkbench((state) => state.closeSettings);
  const returnTo = useWorkbench((state) => state.settingsReturn);

  // Without accounts (a server built without them) everything is open.
  const sections = SETTINGS_SECTIONS.filter(
    (s) => s.minRole === null || !user || hasRole(user.role, s.minRole)
  );
  const section = sections.find((s) => s.id === current) ?? sections[0];
  const Body = section?.component;

  return (
    <div className="portal" data-testid="settings-view">
      <aside className="portal__side">
        <div className="portal__brand">
          <img src="/favicon.svg" alt="" />
          <div>
            <div className="portal__product">Settings</div>
            {user && (
              <div className="portal__version">
                {user.display_name} · {user.role}
              </div>
            )}
          </div>
        </div>
        <nav className="portal__nav" aria-label="Settings">
          {sections.map((s) => {
            const Icon = s.icon;
            return (
              <button
                key={s.id}
                type="button"
                className={`portal__nav-item${s.id === section?.id ? ' is-active' : ''}`}
                aria-current={s.id === section?.id}
                onClick={() => showSettings(s.id)}
              >
                <Icon size={16} aria-hidden /> {s.label}
              </button>
            );
          })}
        </nav>
        {user && (
          <button type="button" className="portal__back" onClick={() => void signOut()}>
            <LogOut size={14} aria-hidden /> Sign out
          </button>
        )}
        <button type="button" className="portal__back" onClick={closeSettings}>
          <ArrowLeft size={14} aria-hidden /> Back to {returnTo === 'project' ? 'the project' : 'the start page'}
        </button>
      </aside>
      <main className="portal__main">
        <section className="portal__panel">{Body && <Body />}</section>
      </main>
    </div>
  );
}
