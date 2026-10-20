import React from 'react';
import { describe, it, expect } from 'vitest';
import { renderToString } from 'react-dom/server';
import { MythosLogo } from '../components/icons/MythosLogo';
import { TopBar } from '../components/TopBar';
import { PortalView } from '../components/portal/PortalView';

describe('MythosLogo Component & Official Branding', () => {
  it('renders standalone MythosLogo with default attributes', () => {
    const html = renderToString(<MythosLogo />);
    expect(html).toContain('<svg');
    expect(html).toContain('viewBox="0 0 128 128"');
    expect(html).toContain('aria-label="MythOS Logo"');
    expect(html).toContain('width="24"');
    expect(html).toContain('height="24"');
  });

  it('renders MythosLogo with custom size and background', () => {
    const html = renderToString(<MythosLogo size={48} showBackground />);
    expect(html).toContain('width="48"');
    expect(html).toContain('height="48"');
    expect(html).toContain('rx="28"');
    expect(html).toContain('stroke="#1e293b"');
  });

  it('renders TopBar with official MythosLogo instead of mock letter box', () => {
    const html = renderToString(<TopBar />);
    expect(html).toContain('aria-label="MythOS Logo"');
    expect(html).not.toContain('>M<');
    expect(html).not.toContain('Siemens');
  });

  it('renders PortalView with official MythosLogo and purged competitor text', () => {
    const html = renderToString(<PortalView />);
    expect(html).toContain('aria-label="MythOS Logo"');
    expect(html).not.toContain('>M<');
    expect(html).toContain('Industrial Automation Suite');
    expect(html).toContain('MythOS Systems');
    expect(html).not.toContain('Siemens TIA Portal Parity');
    expect(html).not.toContain('Siemens/MythOS');
    expect(html).not.toContain('Siemens');
  });
});
