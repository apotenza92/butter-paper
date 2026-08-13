export function ConstructionGridOverlay({ pageIndex, width, height, spacing }: {
  pageIndex: number;
  width: number;
  height: number;
  spacing: number;
}) {
  const patternId = `construction-grid-pattern-${pageIndex}`;
  return (
    <svg
      className="pointer-events-none absolute inset-0 size-full"
      viewBox={`0 0 ${width} ${height}`}
      aria-hidden="true"
      data-domain-ui-exception="construction-grid-overlay"
      data-testid={`construction-grid-${pageIndex}`}
    >
      <defs>
        <pattern id={patternId} width={spacing} height={spacing} patternUnits="userSpaceOnUse">
          <path d={`M ${spacing} 0 L 0 0 0 ${spacing}`} fill="none" stroke="currentColor" strokeOpacity="0.2" strokeWidth="0.75" vectorEffect="non-scaling-stroke" />
        </pattern>
      </defs>
      <rect width="100%" height="100%" fill={`url(#${patternId})`} />
    </svg>
  );
}
