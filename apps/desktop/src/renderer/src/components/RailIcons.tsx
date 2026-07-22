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
  MoveHorizontal,
  PenLine,
  Route,
  Ruler,
  Waypoints,
  Pentagon,
  Spline,
  Square,
  Type,
} from 'lucide-react';
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
    <MoveHorizontal
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
    <Ruler
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
    <span aria-hidden="true" className="relative inline-flex items-center justify-center">
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
        className="absolute -right-0.5 -top-0.5"
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
    <span aria-hidden="true" className="relative inline-flex items-center justify-center">
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
        className="absolute"
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
