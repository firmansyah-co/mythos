/**
 * MythOS W3C DTCG Design Token Parser & Typed Accessor
 * Standards: ANSI/ISA-101.01-2015, ANSI/ISA-18.2-2016, ISO 9241-110, WCAG 2.1 AAA
 */

export interface DtcgTokenValue<T = string> {
  $value: T;
  $type: string;
  $description?: string;
  $extensions?: {
    mode?: {
      dark: T;
      light: T;
    };
    [key: string]: unknown;
  };
}

export type ThemeMode = 'dark' | 'light';

export interface ThemeColors {
  bgBase: string;
  bgSurface: string;
  bgCard: string;
  bgElevated: string;
  bgMuted: string;
  bgHighlight: string;
  borderSubtle: string;
  borderDefault: string;
  borderFocus: string;
  textPrimary: string;
  textSecondary: string;
  textMuted: string;
  textInverse: string;
  alarmCritical: string;
  alarmMajor: string;
  alarmMinor: string;
  alarmDiagnostic: string;
  alarmNormal: string;
  alarmShelved: string;
  busEthernet: string;
  busSerial: string;
  busCan: string;
  busSafety: string;
  busIoLink: string;
  busOffline: string;
}

export const DARK_THEME_TOKENS: ThemeColors = {
  bgBase: '#0A0D12',
  bgSurface: '#0F141C',
  bgCard: '#161D29',
  bgElevated: '#161E2E',
  bgMuted: '#1E293B',
  bgHighlight: '#334155',
  borderSubtle: '#1E293B',
  borderDefault: '#334155',
  borderFocus: '#38BDF8',
  textPrimary: '#F8FAFC',
  textSecondary: '#94A3B8',
  textMuted: '#64748B',
  textInverse: '#020617',
  alarmCritical: '#DC2626',
  alarmMajor: '#EA580C',
  alarmMinor: '#CA8A04',
  alarmDiagnostic: '#2563EB',
  alarmNormal: '#475569',
  alarmShelved: '#78716C',
  busEthernet: '#16A34A',
  busSerial: '#0284C7',
  busCan: '#CA8A04',
  busSafety: '#DC2626',
  busIoLink: '#9333EA',
  busOffline: '#475569',
};

export const LIGHT_THEME_TOKENS: ThemeColors = {
  bgBase: '#F8FAFC',
  bgSurface: '#FFFFFF',
  bgCard: '#F1F5F9',
  bgElevated: '#F1F5F9',
  bgMuted: '#E2E8F0',
  bgHighlight: '#CBD5E1',
  borderSubtle: '#E2E8F0',
  borderDefault: '#CBD5E1',
  borderFocus: '#0284C7',
  textPrimary: '#0F172A',
  textSecondary: '#475569',
  textMuted: '#64748B',
  textInverse: '#FFFFFF',
  alarmCritical: '#DC2626',
  alarmMajor: '#D97706',
  alarmMinor: '#CA8A04',
  alarmDiagnostic: '#2563EB',
  alarmNormal: '#64748B',
  alarmShelved: '#94A3B8',
  busEthernet: '#15803D',
  busSerial: '#0369A1',
  busCan: '#B45309',
  busSafety: '#DC2626',
  busIoLink: '#7E22CE',
  busOffline: '#64748B',
};

export const SPATIAL_TOKENS = {
  hudHeight: '38px',
  activityBarWidth: '48px',
  statusBarHeight: '24px',
  inspectorWidth: '384px',
  inspectorFixedWidth: '384px',
  primaryExplorerMinWidth: '240px',
  primaryExplorerMaxWidth: '480px',
  primaryExplorerDefaultWidth: '280px',
  consoleMinHeight: '160px',
  consoleDefaultHeight: '220px',
  gridSnap: '16px',
  gridSnap8: '8px',
  gridSnap16: '16px',
  canvasStandardWidth: '1920px',
  canvasStandardHeight: '1080px',
  canvas4kMaxWidth: '3840px',
  canvas4kMaxHeight: '2160px',
} as const;

export const TYPOGRAPHY_TOKENS = {
  fontMono: "'JetBrains Mono', 'Fira Code', 'Roboto Mono', ui-monospace, monospace",
  fontSans: "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
  fontSizeMicro: '10px',
  fontSizeCaption: '11px',
  fontSizeBody: '13px',
  fontSizeHeaderSm: '14px',
  fontSizeHeaderMd: '16px',
  fontSizeHeaderLg: '20px',
  fontSizeTelemetryDisplay: '28px',
  fontWeightRegular: 400,
  fontWeightMedium: 500,
  fontWeightSemibold: 600,
  fontWeightBold: 700,
} as const;

/**
 * Calculates sRGB relative luminance according to WCAG 2.1 specifications
 */
export function getLuminance(hexColor: string): number {
  const hex = hexColor.replace(/^#/, '');
  const r = parseInt(hex.substring(0, 2), 16) / 255;
  const g = parseInt(hex.substring(2, 4), 16) / 255;
  const b = parseInt(hex.substring(4, 6), 16) / 255;

  const toLinear = (c: number) =>
    c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);

  return 0.2126 * toLinear(r) + 0.7152 * toLinear(g) + 0.0722 * toLinear(b);
}

/**
 * Computes the WCAG 2.1 contrast ratio between two hex colors (L1 + 0.05) / (L2 + 0.05)
 */
export function getContrastRatio(fgHex: string, bgHex: string): number {
  const l1 = getLuminance(fgHex);
  const l2 = getLuminance(bgHex);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

/**
 * Validates WCAG AAA compliance (contrast ratio >= 7.0:1 for normal text, >= 4.5:1 for large text)
 */
export function isWcagAaaCompliant(fgHex: string, bgHex: string, isLargeText: boolean = false): boolean {
  const ratio = getContrastRatio(fgHex, bgHex);
  return isLargeText ? ratio >= 4.5 : ratio >= 7.0;
}

/**
 * Resolve theme color by token key and current active mode
 */
export function getThemeToken(tokenKey: keyof ThemeColors, mode: ThemeMode = 'dark'): string {
  return mode === 'dark' ? DARK_THEME_TOKENS[tokenKey] : LIGHT_THEME_TOKENS[tokenKey];
}

/**
 * DTCG Json Token Parser: Traverses W3C DTCG token structure and produces flattened CSS map
 */
export function parseDtcgTokens(rawJson: Record<string, unknown>, mode: ThemeMode = 'dark'): Record<string, string> {
  const result: Record<string, string> = {};

  function traverse(node: unknown, path: string[]) {
    if (!node || typeof node !== 'object') return;
    const obj = node as Record<string, unknown>;

    if ('$value' in obj) {
      const varName = '--' + path.join('-');
      // Check if mode extensions exist
      const ext = obj.$extensions as Record<string, unknown> | undefined;
      const modeExt = ext?.mode as Record<string, string> | undefined;

      if (modeExt && mode in modeExt) {
        result[varName] = modeExt[mode];
      } else if (typeof obj.$value === 'string' && !obj.$value.startsWith('{')) {
        result[varName] = obj.$value;
      }
      return;
    }

    for (const [key, value] of Object.entries(obj)) {
      if (!key.startsWith('$')) {
        traverse(value, [...path, key]);
      }
    }
  }

  traverse(rawJson, []);
  return result;
}
