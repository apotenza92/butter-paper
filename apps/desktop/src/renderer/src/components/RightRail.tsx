import type { ReactNode } from 'react';
import type { ToolMode } from '../../../shared/protocol';
import { Toggle } from '@/components/ui/toggle';
import { PDF_TOOL_REGISTRY } from '../pdf-tools/toolRegistry';
import {
  PanRailIcon,
  ArcRailIcon,
  RectangleRailIcon,
  EllipseRailIcon,
  LineRailIcon,
  LengthRailIcon,
  PolylengthRailIcon,
  AreaRailIcon,
  ArrowRailIcon,
  CalloutRailIcon,
  CloudPlusRailIcon,
  CloudRailIcon,
  DimensionRailIcon,
  HighlightRailIcon,
  ImageRailIcon,
  PolylineRailIcon,
  PolygonRailIcon,
  PenRailIcon,
  SelectRailIcon,
  SnapshotRailIcon,
  TextBoxRailIcon,
} from './RailIcons';
import { RailScrollArea } from './RailScrollArea';
import {
  RAIL_INSET,
  RAIL_WIDTH,
  SHELL_SURFACE_APP,
} from './shellSpacing';

interface RightRailProps {
  activeTool: ToolMode;
  disabled?: boolean;
  onSelectTool: (tool: ToolMode) => void;
}

const TOOL_ICONS: Record<ToolMode, ReactNode> = {
  select: <SelectRailIcon />,
  pan: <PanRailIcon />,
  'text-box': <TextBoxRailIcon />,
  rectangle: <RectangleRailIcon />,
  ellipse: <EllipseRailIcon />,
  arc: <ArcRailIcon />,
  line: <LineRailIcon />,
  arrow: <ArrowRailIcon />,
  dimension: <DimensionRailIcon />,
  length: <LengthRailIcon />,
  polylength: <PolylengthRailIcon />,
  area: <AreaRailIcon />,
  polyline: <PolylineRailIcon />,
  polygon: <PolygonRailIcon />,
  pen: <PenRailIcon />,
  highlight: <HighlightRailIcon />,
  cloud: <CloudRailIcon />,
  'cloud-plus': <CloudPlusRailIcon />,
  callout: <CalloutRailIcon />,
  image: <ImageRailIcon />,
  snapshot: <SnapshotRailIcon />,
};

function RailToolButton({
  active,
  disabled,
  label,
  shortcut,
  icon,
  testId,
  onClick,
}: {
  active: boolean;
  disabled?: boolean;
  label: string;
  shortcut?: string;
  icon: ReactNode;
  testId: string;
  onClick: () => void;
}) {
  const accessibleLabel = shortcut ? `${label} (${shortcut})` : label;
  return (
    <Toggle
      type="button"
      variant="outline"
      pressed={active}
      data-testid={testId}
      data-rail-tooltip={accessibleLabel}
      aria-label={label}
      disabled={disabled}
      className="group relative size-8 shrink-0 rounded-2xl p-0"
      onPressedChange={onClick}
    >
      {icon}
    </Toggle>
  );
}

export function RightRail({ activeTool, disabled = false, onSelectTool }: RightRailProps) {
  return (
    <aside
      className={['bp-border-left-inset flex h-full min-h-0 flex-col items-center', RAIL_WIDTH, RAIL_INSET, SHELL_SURFACE_APP].join(' ')}
      data-testid="right-rail"
    >
      <RailScrollArea
        overflowIndicatorTestId="right-rail-overflow-indicator"
        overflowSide="left"
      >
        {PDF_TOOL_REGISTRY.map((tool) => (
          <RailToolButton
            key={tool.id}
            active={activeTool === tool.id}
            label={tool.label}
            shortcut={tool.shortcut}
            disabled={disabled}
            icon={TOOL_ICONS[tool.id]}
            testId={tool.testId}
            onClick={() => onSelectTool(tool.id)}
          />
        ))}
      </RailScrollArea>
    </aside>
  );
}
