/** Minimal inline icons (decorative; buttons carry translated aria-labels). */
const base = {
  width: 18,
  height: 18,
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 2,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
  'aria-hidden': true,
};

export const IconSelect = () => (
  <svg {...base}>
    <path d="M5 3l14 8-6 2-2 6z" />
  </svg>
);
export const IconBox = () => (
  <svg {...base}>
    <rect x="4" y="4" width="16" height="16" strokeDasharray="3 3" />
  </svg>
);
export const IconMove = () => (
  <svg {...base}>
    <path d="M12 3v18M3 12h18M12 3l-3 3M12 3l3 3M12 21l-3-3M12 21l3-3M3 12l3-3M3 12l3 3M21 12l-3-3M21 12l-3 3" />
  </svg>
);
export const IconUndo = () => (
  <svg {...base}>
    <path d="M9 14L4 9l5-5" />
    <path d="M4 9h10a6 6 0 010 12h-3" />
  </svg>
);
export const IconRedo = () => (
  <svg {...base}>
    <path d="M15 14l5-5-5-5" />
    <path d="M20 9H10a6 6 0 000 12h3" />
  </svg>
);
export const IconGauge = () => (
  <svg {...base}>
    <path d="M4 18a8 8 0 1116 0" />
    <path d="M12 18l4-6" />
  </svg>
);
export const IconDelete = () => (
  <svg {...base}>
    <path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3" />
  </svg>
);
export const IconFit = () => (
  <svg {...base}>
    <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />
  </svg>
);
