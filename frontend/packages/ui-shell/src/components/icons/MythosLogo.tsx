import React, { useId } from 'react';

export interface MythosLogoProps extends React.SVGProps<SVGSVGElement> {
  size?: number | string;
  className?: string;
  showBackground?: boolean;
  glow?: boolean;
}

/**
 * Official MythOS Brand Logo Component
 * High-fidelity vector rendering of the precision crucible "M" and industrial styling.
 */
export const MythosLogo: React.FC<MythosLogoProps> = ({
  size = 24,
  className = '',
  showBackground = false,
  glow = true,
  ...props
}) => {
  const rawId = useId();
  // Sanitize useId (colons can cause issues in SVG url(#id) in some environments)
  const idPrefix = 'mythos-logo-' + rawId.replace(/[^a-zA-Z0-9_-]/g, '');

  const bgGlowId = `${idPrefix}-bg-glow`;
  const gradPrimaryId = `${idPrefix}-grad-primary`;
  const gradSecondaryId = `${idPrefix}-grad-secondary`;
  const coreGlowId = `${idPrefix}-core-glow`;

  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 128 128"
      width={size}
      height={size}
      className={`inline-block shrink-0 ${className}`}
      aria-label="MythOS Logo"
      role="img"
      {...props}
    >
      <defs>
        {/* Background Radial Glow */}
        {showBackground && (
          <radialGradient id={bgGlowId} cx="50%" cy="50%" r="70%">
            <stop offset="0%" stopColor="var(--brand-shadow)" />
            <stop offset="100%" stopColor="var(--brand-shadow-deep)" />
          </radialGradient>
        )}

        {/* Logo Gradient: Electric Amber to Vivid Rust */}
        <linearGradient id={gradPrimaryId} x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="var(--brand-flame-light)" />
          <stop offset="50%" stopColor="var(--brand-flame-mid)" />
          <stop offset="100%" stopColor="var(--brand-flame-deep)" />
        </linearGradient>

        {/* Logo Gradient: Cyan Pulse Accent */}
        <linearGradient id={gradSecondaryId} x1="0%" y1="100%" x2="100%" y2="0%">
          <stop offset="0%" stopColor="var(--brand-cyan-deep)" />
          <stop offset="100%" stopColor="var(--brand-cyan-light)" />
        </linearGradient>

        {/* Soft Ambient Glow Filter */}
        {glow && (
          <filter id={coreGlowId} x="-20%" y="-20%" width="140%" height="140%">
            <feGaussianBlur stdDeviation="4" result="blur" />
            <feComposite in="SourceGraphic" in2="blur" operator="over" />
          </filter>
        )}
      </defs>

      {/* Rounded App Icon Background */}
      {showBackground && (
        <>
          <rect
            width="128"
            height="128"
            rx="28"
            fill={`url(#${bgGlowId})`}
            stroke="var(--border-subtle)"
            strokeWidth="2"
          />

          {/* Subtle Blueprint Grid Pattern */}
          <g opacity="0.06" stroke="var(--text-secondary)" strokeWidth="1">
            <path d="M 0 32 L 128 32 M 0 64 L 128 64 M 0 96 L 128 96" />
            <path d="M 32 0 L 32 128 M 64 0 L 64 128 M 96 0 L 96 128" />
          </g>
        </>
      )}

      {/* Centered Composition */}
      <g transform="translate(64, 64)">
        {/* Ambient Halo Glow behind Logo */}
        {glow && (
          <>
            <circle
              cx="0"
              cy="0"
              r="42"
              fill="var(--brand-orange)"
              opacity="0.12"
              filter={`url(#${coreGlowId})`}
            />
            <circle
              cx="0"
              cy="0"
              r="24"
              fill="var(--border-focus)"
              opacity="0.10"
              filter={`url(#${coreGlowId})`}
            />
          </>
        )}

        {/* MINIMALIST GEOMETRIC ICON: PRECISION CRUCIBLE "M" */}
        <g transform="translate(-40, -42) scale(0.80)">
          {/* Left Pylon */}
          <path
            d="M 12 100 L 12 24 L 38 48 L 38 100 Z"
            fill={`url(#${gradPrimaryId})`}
          />

          {/* Right Pylon */}
          <path
            d="M 88 100 L 88 24 L 62 48 L 62 100 Z"
            fill={`url(#${gradPrimaryId})`}
          />

          {/* Center Dynamic Chevron / Core Node */}
          <path
            d="M 50 16 L 72 38 L 50 60 L 28 38 Z"
            fill={`url(#${gradSecondaryId})`}
            filter={glow ? `url(#${coreGlowId})` : undefined}
          />
          <path
            d="M 50 24 L 64 38 L 50 52 L 36 38 Z"
            fill="var(--text-primary)"
            opacity="0.95"
          />
        </g>
      </g>
    </svg>
  );
};

export default MythosLogo;
