import React from 'react';
import type { ProcessDataQuality } from '../types';
import { dataQualityLabel, normalizeQuality } from '../processData';

/**
 * DataQualityBadge — explicit ANSI/ISA-101 annunciation of where a faceplate's values come from.
 *
 * Renders nothing for live data. `NOT BOUND` is a configuration state and stays neutral grey;
 * `BAD COMM FAILURE` is abnormal and is the only state that uses a saturated (amber) tone.
 * The badge is what makes a quiescent faceplate unmistakably different from a live one.
 */
export const DataQualityBadge: React.FC<{ quality?: ProcessDataQuality | null; className?: string }> = ({
  quality,
  className = '',
}) => {
  const label = dataQualityLabel(quality);
  if (!label) return null;

  const resolved = normalizeQuality(quality);
  const isBadQuality = resolved === 'BAD_QUALITY';

  const tone = isBadQuality
    ? 'bg-amber-950/70 border-amber-700 text-amber-300'
    : 'bg-neutral-800/80 border-neutral-600 text-neutral-300';

  return (
    <span
      data-testid="faceplate-no-data"
      data-quality={resolved}
      title={
        isBadQuality
          ? 'The bound device is not reporting usable data. Process values are suppressed.'
          : 'No device binding. Process values are suppressed.'
      }
      className={`px-1.5 py-0.5 rounded border text-[10px] font-mono font-semibold tracking-wider whitespace-nowrap ${tone} ${className}`}
    >
      {label}
    </span>
  );
};
