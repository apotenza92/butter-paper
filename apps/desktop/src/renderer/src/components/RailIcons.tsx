import {
  Files,
  Hand,
  MessageSquare,
  MousePointer2,
  Circle,
  ArrowRight,
  ChartArea,
  Cloud,
  Highlighter,
  Image,
  ScanSearch,
  Minus,
  PenLine,
  Route,
  Ruler,
  RulerDimensionLine,
  ShieldX,
  Waypoints,
  Pentagon,
  Spline,
  Square,
  Type,
} from 'lucide-react';
import type { ComponentType } from 'react';
import type { ToolMode } from '../../../shared/protocol';
import {
  CONTROL_ICON_SIZE,
  CONTROL_ICON_SIZE_CLASS,
  CONTROL_ICON_STROKE_WIDTH,
} from './shellSpacing';

interface RailIconProps {
  className?: string;
}

function railIconClassName(className?: string) {
  return [CONTROL_ICON_SIZE_CLASS, className].filter(Boolean).join(' ');
}

export function PagesRailIcon({ className }: RailIconProps) {
  return (
    <Files
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function SelectRailIcon({ className }: RailIconProps) {
  return (
    <MousePointer2
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function PanRailIcon({ className }: RailIconProps) {
  return (
    <Hand
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function RectangleRailIcon({ className }: RailIconProps) {
  return (
    <Square
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function EllipseRailIcon({ className }: RailIconProps) {
  return (
    <Circle
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function ArcRailIcon({ className }: RailIconProps) {
  return (
    <Spline
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function LineRailIcon({ className }: RailIconProps) {
  return (
    <Minus
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function ArrowRailIcon({ className }: RailIconProps) {
  return (
    <ArrowRight
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function DimensionRailIcon({ className }: RailIconProps) {
  return (
    <Ruler
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function LengthRailIcon({ className }: RailIconProps) {
  return (
    <RulerDimensionLine
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function PolylengthRailIcon({ className }: RailIconProps) {
  return (
    <Route
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function AreaRailIcon({ className }: RailIconProps) {
  return (
    <ChartArea
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function PolylineRailIcon({ className }: RailIconProps) {
  return (
    <Waypoints
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function PolygonRailIcon({ className }: RailIconProps) {
  return (
    <Pentagon
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function PenRailIcon({ className }: RailIconProps) {
  return (
    <PenLine
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function HighlightRailIcon({ className }: RailIconProps) {
  return (
    <Highlighter
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function CloudRailIcon({ className }: RailIconProps) {
  return (
    <Cloud
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function CloudPlusRailIcon({ className }: RailIconProps) {
  return (
    <span aria-hidden="true" className="relative inline-flex size-4 items-center justify-center">
      <Cloud
        size={CONTROL_ICON_SIZE}
        strokeWidth={CONTROL_ICON_STROKE_WIDTH}
        absoluteStrokeWidth
        className={railIconClassName(className)}
      />
      <Type
        size={Math.round(CONTROL_ICON_SIZE * 0.42)}
        strokeWidth={CONTROL_ICON_STROKE_WIDTH}
        absoluteStrokeWidth
        className="absolute -right-0.5 -top-0.5 size-[7px]"
      />
    </span>
  );
}

export function TextBoxRailIcon({ className }: RailIconProps) {
  return (
    <Type
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function CalloutRailIcon({ className }: RailIconProps) {
  return (
    <span aria-hidden="true" className="relative inline-flex size-4 items-center justify-center">
      <MessageSquare
        size={CONTROL_ICON_SIZE}
        strokeWidth={CONTROL_ICON_STROKE_WIDTH}
        absoluteStrokeWidth
        className={railIconClassName(className)}
      />
      <Type
        size={Math.round(CONTROL_ICON_SIZE * 0.4)}
        strokeWidth={CONTROL_ICON_STROKE_WIDTH}
        absoluteStrokeWidth
        className="absolute size-[6px]"
      />
    </span>
  );
}

export function ImageRailIcon({ className }: RailIconProps) {
  return (
    <Image
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function RedactRailIcon({ className }: RailIconProps) {
  return (
    <ShieldX
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

export function SnapshotRailIcon({ className }: RailIconProps) {
  return (
    <ScanSearch
      aria-hidden="true"
      size={CONTROL_ICON_SIZE}
      strokeWidth={CONTROL_ICON_STROKE_WIDTH}
      absoluteStrokeWidth
      className={railIconClassName(className)}
    />
  );
}

const TOOL_RAIL_ICON_COMPONENTS = {
  select: SelectRailIcon,
  pan: PanRailIcon,
  'text-box': TextBoxRailIcon,
  rectangle: RectangleRailIcon,
  ellipse: EllipseRailIcon,
  arc: ArcRailIcon,
  line: LineRailIcon,
  arrow: ArrowRailIcon,
  dimension: DimensionRailIcon,
  length: LengthRailIcon,
  polylength: PolylengthRailIcon,
  area: AreaRailIcon,
  polyline: PolylineRailIcon,
  polygon: PolygonRailIcon,
  pen: PenRailIcon,
  highlight: HighlightRailIcon,
  cloud: CloudRailIcon,
  'cloud-plus': CloudPlusRailIcon,
  callout: CalloutRailIcon,
  redact: RedactRailIcon,
  image: ImageRailIcon,
  snapshot: SnapshotRailIcon,
} satisfies Record<ToolMode, ComponentType<RailIconProps>>;

export function ToolRailIcon({ tool, className }: RailIconProps & { readonly tool: ToolMode }) {
  const Icon = TOOL_RAIL_ICON_COMPONENTS[tool];
  return <Icon className={className} />;
}
