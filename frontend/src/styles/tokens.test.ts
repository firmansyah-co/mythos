import { describe, it, expect } from 'vitest';
import {
  DARK_THEME_TOKENS,
  LIGHT_THEME_TOKENS,
  SPATIAL_TOKENS,
  getContrastRatio,
  isWcagAaaCompliant,
  parseDtcgTokens,
} from './tokens';
import dtcgRaw from '../../../docs/04-hmi-and-ui/design-tokens-dtcg.json';

describe('TSK-301: W3C DTCG Design Token Architecture & Parser', () => {
  it('should define dark obsidian and engineering clean slate tokens', () => {
    expect(DARK_THEME_TOKENS.bgBase).toBe('#0A0D12');
    expect(DARK_THEME_TOKENS.bgSurface).toBe('#0F141C');
    expect(DARK_THEME_TOKENS.bgCard).toBe('#161D29');
    expect(DARK_THEME_TOKENS.textPrimary).toBe('#F8FAFC');

    expect(LIGHT_THEME_TOKENS.bgBase).toBe('#F8FAFC');
    expect(LIGHT_THEME_TOKENS.bgSurface).toBe('#FFFFFF');
    expect(LIGHT_THEME_TOKENS.textPrimary).toBe('#0F172A');
  });

  it('should define ISA-101 semantic alarm tokens correctly', () => {
    expect(DARK_THEME_TOKENS.alarmCritical).toBe('#DC2626');
    expect(DARK_THEME_TOKENS.alarmMajor).toBe('#EA580C');
    expect(DARK_THEME_TOKENS.alarmMinor).toBe('#CA8A04');
    expect(DARK_THEME_TOKENS.alarmDiagnostic).toBe('#2563EB');
    expect(DARK_THEME_TOKENS.alarmNormal).toBe('#475569');
  });

  it('should define spatial layout tokens adhering to Adrian box-model invariants', () => {
    expect(SPATIAL_TOKENS.hudHeight).toBe('38px');
    expect(SPATIAL_TOKENS.activityBarWidth).toBe('48px');
    expect(SPATIAL_TOKENS.statusBarHeight).toBe('24px');
    expect(SPATIAL_TOKENS.inspectorWidth).toBe('384px');
    expect(SPATIAL_TOKENS.inspectorFixedWidth).toBe('384px');
    expect(SPATIAL_TOKENS.gridSnap).toBe('16px');
  });

  it('should enforce WCAG AAA contrast ratio > 7:1 for text on dark backgrounds', () => {
    const contrastBase = getContrastRatio(DARK_THEME_TOKENS.textPrimary, DARK_THEME_TOKENS.bgBase);
    const contrastSurface = getContrastRatio(DARK_THEME_TOKENS.textPrimary, DARK_THEME_TOKENS.bgSurface);
    const contrastCard = getContrastRatio(DARK_THEME_TOKENS.textPrimary, DARK_THEME_TOKENS.bgCard);

    expect(contrastBase).toBeGreaterThan(7.0);
    expect(contrastSurface).toBeGreaterThan(7.0);
    expect(contrastCard).toBeGreaterThan(7.0);

    expect(isWcagAaaCompliant(DARK_THEME_TOKENS.textPrimary, DARK_THEME_TOKENS.bgBase)).toBe(true);
    expect(isWcagAaaCompliant(DARK_THEME_TOKENS.textPrimary, DARK_THEME_TOKENS.bgSurface)).toBe(true);
  });

  it('should enforce WCAG AAA contrast ratio > 7:1 for text on light backgrounds', () => {
    const contrastLightBase = getContrastRatio(LIGHT_THEME_TOKENS.textPrimary, LIGHT_THEME_TOKENS.bgBase);
    const contrastLightSurface = getContrastRatio(LIGHT_THEME_TOKENS.textPrimary, LIGHT_THEME_TOKENS.bgSurface);

    expect(contrastLightBase).toBeGreaterThan(7.0);
    expect(contrastLightSurface).toBeGreaterThan(7.0);

    expect(isWcagAaaCompliant(LIGHT_THEME_TOKENS.textPrimary, LIGHT_THEME_TOKENS.bgBase)).toBe(true);
    expect(isWcagAaaCompliant(LIGHT_THEME_TOKENS.textPrimary, LIGHT_THEME_TOKENS.bgSurface)).toBe(true);
  });

  it('should parse W3C DTCG json token structure into CSS variable map', () => {
    const darkCssMap = parseDtcgTokens(dtcgRaw as unknown as Record<string, unknown>, 'dark');
    expect(darkCssMap['--color-background-base']).toBe('#0A0D12');
    expect(darkCssMap['--color-background-surface']).toBe('#0F141C');
    expect(darkCssMap['--color-isa101-alarm-critical']).toBe('#DC2626');
    expect(darkCssMap['--dimension-hud-height']).toBe('38px');
    expect(darkCssMap['--dimension-inspector-fixed-width']).toBe('384px');

    const lightCssMap = parseDtcgTokens(dtcgRaw as unknown as Record<string, unknown>, 'light');
    expect(lightCssMap['--color-background-base']).toBe('#F8FAFC');
    expect(lightCssMap['--color-background-surface']).toBe('#FFFFFF');
  });
});
