/**
 * Design tokens. Mirrors the CSS custom properties defined in `tokens.css`; use the CSS
 * variables in styles and this object where a raw value is needed (e.g. three.js colours).
 */
export const tokens = {
  font: {
    heading: 'Georgia, "Gelasio", serif',
    body: '"Inter", system-ui, sans-serif',
  },
  color: {
    accent: '#FF6727',
    accentHover: '#E5551A',
    onAccent: '#1B1B1F',
    /** Exact colour of selected modules in the 3D viewport. */
    selection3d: '#FF6727',
  },
  light: {
    bg: '#F4F4F5',
    surface: '#FFFFFF',
    surfaceRaised: '#FAFAFA',
    border: '#D9D9DE',
    text: '#1B1B1F',
    textMuted: '#5E5E66',
    viewportBg: '#DCE3EA',
  },
  dark: {
    bg: '#141416',
    surface: '#1D1D20',
    surfaceRaised: '#26262A',
    border: '#38383E',
    text: '#EDEDEF',
    textMuted: '#A0A0A8',
    viewportBg: '#202328',
  },
  radius: { sm: '4px', md: '6px', lg: '10px' },
  space: { xs: '4px', sm: '8px', md: '12px', lg: '16px', xl: '24px' },
} as const;

export type Theme = 'light' | 'dark';
