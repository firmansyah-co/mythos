import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { TopBar } from '../components/TopBar';
import { PresenceAvatarStack } from '../components/collab/PresenceAvatarStack';
import { useCollabStore } from '../collab/useCollabStore';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('TopBar & PresenceAvatarStack (TSK-311 Collaboration UI)', () => {
  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);

    useCollabStore.setState({
      connected: false,
      peers: [],
    });
  });

  afterEach(() => {
    flushSync(() => {
      root.unmount();
    });
    container.remove();
  });

  it('renders TopBar with Brand and PresenceAvatarStack component', () => {
    const html = renderClient(<TopBar onOpenExitDialog={() => {}} />);
    expect(html).toContain('MythOS');
    expect(html).toContain('● 0 Online');
    expect(container.querySelector('[data-testid="presence-avatar-stack"]')).toBeTruthy();
  });

  it('renders live presence avatar stack with peer initials and online status badge', () => {
    useCollabStore.setState({
      connected: true,
      peers: [
        {
          clientId: 101,
          user: { name: 'Neo Software', color: '#10B981' },
          activeTab: 'MainLogic.st',
          activeScreen: 'screen_01_plant_overview',
        },
        {
          clientId: 102,
          user: { name: 'Iris Design', color: '#EC4899' },
          activeTab: 'CanvasHMI',
          activeScreen: 'screen_02_bottling',
        },
        {
          clientId: 103,
          user: { name: 'Vulcan Hardware', color: '#F59E0B' },
          activeTab: 'IO_Diagnostics',
          activeScreen: 'screen_03_diagnostics',
        },
      ],
    });

    const html = renderClient(<PresenceAvatarStack />);
    expect(html).toContain('● 3 Online');

    const avatarNeo = container.querySelector('[data-testid="peer-avatar-101"]');
    const avatarIris = container.querySelector('[data-testid="peer-avatar-102"]');
    const avatarVulcan = container.querySelector('[data-testid="peer-avatar-103"]');

    expect(avatarNeo).toBeTruthy();
    expect(avatarIris).toBeTruthy();
    expect(avatarVulcan).toBeTruthy();

    expect(avatarNeo?.textContent).toBe('NS');
    expect(avatarIris?.textContent).toBe('ID');
    expect(avatarVulcan?.textContent).toBe('VH');
  });

  it('shows tooltip with active user activities on focus/hover', () => {
    useCollabStore.setState({
      connected: true,
      peers: [
        {
          clientId: 201,
          user: { name: 'Alice Engineer', color: '#3B82F6' },
          activeTab: 'BottlingRung.ld',
        },
      ],
    });

    renderClient(<PresenceAvatarStack />);

    const stackContainer = container.querySelector('[data-testid="presence-avatar-stack"]') as HTMLElement;
    expect(stackContainer).toBeTruthy();

    // Trigger focus to show tooltip (works reliably across happy-dom/browsers)
    flushSync(() => {
      stackContainer.focus();
    });

    const tooltip = container.querySelector('[data-testid="presence-tooltip"]');
    expect(tooltip).toBeTruthy();
    expect(tooltip?.textContent).toContain('Active Collaborators');
    expect(tooltip?.textContent).toContain('Alice Engineer');
    expect(tooltip?.textContent).toContain('BottlingRung.ld');

    // Trigger blur to hide tooltip
    flushSync(() => {
      stackContainer.blur();
    });

    expect(container.querySelector('[data-testid="presence-tooltip"]')).toBeNull();
  });

  it('renders overflow counter when more than 5 peers are present', () => {
    const manyPeers = Array.from({ length: 8 }).map((_, i) => ({
      clientId: 1000 + i,
      user: { name: `Eng ${i}`, color: '#3B82F6' },
      activeTab: `Tab ${i}`,
    }));

    useCollabStore.setState({
      connected: true,
      peers: manyPeers,
    });

    renderClient(<PresenceAvatarStack />);

    const overflowBadge = container.querySelector('[data-testid="peer-avatar-overflow"]');
    expect(overflowBadge).toBeTruthy();
    expect(overflowBadge?.textContent).toBe('+3');
  });
});
