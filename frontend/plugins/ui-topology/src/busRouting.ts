import { Position } from '@xyflow/react';

export interface Point {
  x: number;
  y: number;
}

export interface RoutingParams {
  sourceX: number;
  sourceY: number;
  targetX: number;
  targetY: number;
  sourcePosition?: Position;
  targetPosition?: Position;
  gridSize?: number;
  cornerRadius?: number;
}

/**
 * Computes deterministic orthogonal (Manhattan / Bus-style) path points between two ports.
 * Adheres to industrial A* / Orthogonal wiring principles for industrial schematic cleanliness.
 */
export function computeOrthogonalBusPath(params: RoutingParams): string {
  const {
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition = Position.Right,
    targetPosition = Position.Left,
    gridSize = 16,
    cornerRadius = 6,
  } = params;

  // Align to grid if specified
  const sx = sourceX;
  const sy = sourceY;
  const tx = targetX;
  const ty = targetY;

  // Determine standard offset distance from ports
  const stubOffset = 24;

  const points: Point[] = [{ x: sx, y: sy }];

  // Direction offsets based on port attachment position
  const getPortOffset = (x: number, y: number, pos: Position): Point => {
    switch (pos) {
      case Position.Left:
        return { x: x - stubOffset, y };
      case Position.Right:
        return { x: x + stubOffset, y };
      case Position.Top:
        return { x, y: y - stubOffset };
      case Position.Bottom:
        return { x, y: y + stubOffset };
      default:
        return { x: x + stubOffset, y };
    }
  };

  const p1 = getPortOffset(sx, sy, sourcePosition);
  const p4 = getPortOffset(tx, ty, targetPosition);

  points.push(p1);

  // Compute intermediate orthogonal segments
  if (sourcePosition === Position.Right && targetPosition === Position.Left) {
    if (p1.x < p4.x) {
      // Normal left-to-right flow: step at midpoint X
      const midX = Math.round((p1.x + p4.x) / (2 * gridSize)) * gridSize;
      points.push({ x: midX, y: p1.y });
      points.push({ x: midX, y: p4.y });
    } else {
      // Backwards flow: loop above or below depending on Y delta
      const midY = (p1.y + p4.y) / 2;
      const bypassY = Math.round(midY / gridSize) * gridSize + (p1.y <= p4.y ? -48 : 48);
      points.push({ x: p1.x, y: bypassY });
      points.push({ x: p4.x, y: bypassY });
    }
  } else if (sourcePosition === Position.Bottom && targetPosition === Position.Top) {
    if (p1.y < p4.y) {
      const midY = Math.round((p1.y + p4.y) / (2 * gridSize)) * gridSize;
      points.push({ x: p1.x, y: midY });
      points.push({ x: p4.x, y: midY });
    } else {
      const midX = (p1.x + p4.x) / 2;
      const bypassX = Math.round(midX / gridSize) * gridSize + (p1.x <= p4.x ? 48 : -48);
      points.push({ x: bypassX, y: p1.y });
      points.push({ x: bypassX, y: p4.y });
    }
  } else {
    // General case: connect p1 to p4 with L-shape or Z-shape
    const midX = Math.round((p1.x + p4.x) / (2 * gridSize)) * gridSize;
    points.push({ x: midX, y: p1.y });
    points.push({ x: midX, y: p4.y });
  }

  points.push(p4);
  points.push({ x: tx, y: ty });

  return generateSvgPathWithFillets(points, cornerRadius);
}

/**
 * Builds an SVG Path string (M ... L ... Q/A ...) with smooth fillets on right-angle bends
 */
export function generateSvgPathWithFillets(points: Point[], radius: number): string {
  if (points.length < 2) return '';
  if (points.length === 2) {
    return `M ${points[0].x} ${points[0].y} L ${points[1].x} ${points[1].y}`;
  }

  // Remove redundant co-linear points
  const cleaned: Point[] = [points[0]];
  for (let i = 1; i < points.length - 1; i++) {
    const prev = cleaned[cleaned.length - 1];
    const curr = points[i];
    const next = points[i + 1];

    const isColinearX = prev.x === curr.x && curr.x === next.x;
    const isColinearY = prev.y === curr.y && curr.y === next.y;

    if (!isColinearX && !isColinearY) {
      cleaned.push(curr);
    }
  }
  cleaned.push(points[points.length - 1]);

  let path = `M ${cleaned[0].x} ${cleaned[0].y}`;

  for (let i = 1; i < cleaned.length - 1; i++) {
    const pPrev = cleaned[i - 1];
    const pCurr = cleaned[i];
    const pNext = cleaned[i + 1];

    // Vectors
    const d1x = pCurr.x - pPrev.x;
    const d1y = pCurr.y - pPrev.y;
    const len1 = Math.hypot(d1x, d1y);

    const d2x = pNext.x - pCurr.x;
    const d2y = pNext.y - pCurr.y;
    const len2 = Math.hypot(d2x, d2y);

    if (len1 === 0 || len2 === 0) continue;

    const r = Math.min(radius, len1 / 2, len2 / 2);

    const startX = pCurr.x - (d1x / len1) * r;
    const startY = pCurr.y - (d1y / len1) * r;

    const endX = pCurr.x + (d2x / len2) * r;
    const endY = pCurr.y + (d2y / len2) * r;

    path += ` L ${startX} ${startY} Q ${pCurr.x} ${pCurr.y} ${endX} ${endY}`;
  }

  const last = cleaned[cleaned.length - 1];
  path += ` L ${last.x} ${last.y}`;

  return path;
}
