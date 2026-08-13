import { useEffect, useId, useState, type ReactNode } from 'react';
import { AlignCenterIcon, AlignLeftIcon, AlignRightIcon, BoldIcon, ItalicIcon, PlusIcon, StrikethroughIcon, Trash2Icon, UnderlineIcon } from 'lucide-react';
import { HexColorPicker } from 'react-colorful';
import { ANNOTATION_FONT_OPTIONS, annotationFontOption, type AnnotationFontOption } from '@butter-paper/core';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion';
import { Combobox, ComboboxContent, ComboboxEmpty, ComboboxInput, ComboboxItem, ComboboxList } from '@/components/ui/combobox';
import { Button } from '@/components/ui/button';
import { ContextMenu, ContextMenuContent, ContextMenuGroup, ContextMenuItem, ContextMenuTrigger } from '@/components/ui/context-menu';
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel, FieldLegend, FieldSet } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { InputGroup, InputGroupAddon, InputGroupInput, InputGroupText } from '@/components/ui/input-group';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import { Slider } from '@/components/ui/slider';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { DEFAULT_COLOR_PRESETS, hexToHsl, hexToRgb, hslToHex, loadColorPresets, normalizeHexColor, rgbToHex, saveColorPresets } from '../colorPresets';

export interface ControlledPropertyFieldProps<T> {
  label: string;
  value: T;
  mixed?: boolean;
  disabled?: boolean;
  description?: string;
  validation?: string;
  onChange: (value: T) => void;
  onCommit?: (value: T) => void;
}

interface PropertyShellProps {
  label: string;
  mixed?: boolean;
  disabled?: boolean;
  description?: string;
  validation?: string;
  htmlFor?: string;
  className?: string;
  children: ReactNode;
}

function PropertyShell({ label, mixed, disabled, description, validation, htmlFor, className, children }: PropertyShellProps) {
  return (
    <Field className={className} data-disabled={disabled || undefined} data-invalid={Boolean(validation) || undefined}>
      <div className="flex items-center justify-between gap-2">
        <FieldLabel htmlFor={htmlFor} className="text-xs">
          {label}
        </FieldLabel>
        {mixed ? <span className="text-[10px] text-muted-foreground">Mixed</span> : null}
      </div>
      {children}
      {description ? <FieldDescription className="text-xs">{description}</FieldDescription> : null}
      <FieldError>{validation}</FieldError>
    </Field>
  );
}

export function PropertyAccordion({ title, children, defaultOpen = true }: { title: string; children: ReactNode; defaultOpen?: boolean }) {
  return (
    <PropertyDisclosure title={title} defaultOpen={defaultOpen}>
      <div className="flex flex-col gap-5">{children}</div>
    </PropertyDisclosure>
  );
}

function PropertyDisclosure({ title, children, defaultOpen = true }: { title: string; children: ReactNode; defaultOpen?: boolean }) {
  return (
    <Accordion defaultValue={defaultOpen ? [title] : []} className="w-full" data-domain-ui-exception="property-controls">
      <AccordionItem value={title} className="border-b">
        <AccordionTrigger className="rounded-none border-0 px-3 py-3 text-sm font-medium hover:no-underline">{title}</AccordionTrigger>
        <AccordionContent className="px-3 pb-5">{children}</AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}

export function PropertySection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <PropertyDisclosure title={title}>
      <FieldSet className="gap-3">
        <FieldLegend className="sr-only">{title}</FieldLegend>
        <FieldGroup className="grid! grid-cols-2 gap-3">{children}</FieldGroup>
      </FieldSet>
    </PropertyDisclosure>
  );
}

export function ColorPropertyField(props: ControlledPropertyFieldProps<string> & { allowTransparent?: boolean; className?: string }) {
  const id = useId();
  const [colorPresets, setColorPresets] = useState(() => loadColorPresets(window.localStorage));
  const [mode, setMode] = useState<'presets' | 'add'>('presets');
  const [colorFormat, setColorFormat] = useState<'hex' | 'rgb' | 'hsl'>('hex');
  const [draftColor, setDraftColor] = useState(normalizeHexColor(props.value) ?? '#000000');
  const customPresets = colorPresets.filter((color) => !DEFAULT_COLOR_PRESETS.includes(color as (typeof DEFAULT_COLOR_PRESETS)[number]));
  const displayValue = props.mixed ? 'Mixed' : props.value === 'transparent' ? 'Transparent' : props.value.toUpperCase();

  useEffect(() => {
    const handleStorage = (event: StorageEvent) => {
      if (event.key === 'butter-paper.color-presets.v1') setColorPresets(loadColorPresets(window.localStorage));
    };
    window.addEventListener('storage', handleStorage);
    return () => window.removeEventListener('storage', handleStorage);
  }, []);

  const chooseColor = (color: string) => {
    props.onChange(color);
    props.onCommit?.(color);
  };

  const updatePresets = (next: readonly string[]) => {
    const normalized = saveColorPresets(window.localStorage, next);
    setColorPresets(normalized);
  };

  const addPreset = () => {
    const normalized = normalizeHexColor(draftColor);
    if (!normalized) return;
    if (DEFAULT_COLOR_PRESETS.includes(normalized as (typeof DEFAULT_COLOR_PRESETS)[number])) {
      chooseColor(normalized);
      setMode('presets');
      return;
    }
    updatePresets([...colorPresets.filter((preset) => preset !== normalized), normalized]);
    chooseColor(normalized);
    setMode('presets');
  };

  return (
    <PropertyShell {...props} htmlFor={id} className={props.className ?? 'w-full'}>
      <Popover onOpenChange={(open) => !open && setMode('presets')}>
        <Tooltip>
          <TooltipTrigger render={<span className="inline-flex w-full" />}>
            <PopoverTrigger render={<Button id={id} type="button" variant="outline" className="w-full justify-start" disabled={props.disabled} aria-label={`${props.label}: ${displayValue}`} aria-invalid={Boolean(props.validation) || undefined} />}>
              <ColorSwatch color={props.mixed ? 'transparent' : props.value} />
              <span className="truncate">{displayValue}</span>
            </PopoverTrigger>
          </TooltipTrigger>
          <ColorTooltipContent color={props.mixed ? 'transparent' : props.value} />
        </Tooltip>
        <PopoverContent align="start" className="w-[min(17rem,var(--available-width))]" data-domain-ui-exception="property-controls">
          {mode === 'presets' ? (
            <FieldGroup className="gap-3">
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs font-medium">Preset colors</span>
                {props.allowTransparent ? (
                  <Tooltip>
                    <TooltipTrigger render={<Button type="button" variant={props.value === 'transparent' && !props.mixed ? 'outline' : 'ghost'} size="icon" disabled={props.disabled} aria-label="Transparent" aria-pressed={props.value === 'transparent' && !props.mixed} onClick={() => chooseColor('transparent')} />}>
                      <ColorSwatch color="transparent" />
                    </TooltipTrigger>
                    <ColorTooltipContent color="transparent" />
                  </Tooltip>
                ) : null}
              </div>
              <div className="grid grid-cols-6 gap-1.5" role="group" aria-label={`${props.label} default presets`}>
                {DEFAULT_COLOR_PRESETS.map((color) => (
                  <Tooltip key={color}>
                    <TooltipTrigger render={<Button type="button" variant={props.value === color && !props.mixed ? 'outline' : 'ghost'} size="icon" disabled={props.disabled} aria-label={`Use ${color}`} aria-pressed={props.value === color && !props.mixed} onClick={() => chooseColor(color)} />}>
                      <ColorSwatch color={color} />
                    </TooltipTrigger>
                    <ColorTooltipContent color={color} />
                  </Tooltip>
                ))}
              </div>
              <Separator />
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs font-medium">Custom colors</span>
                <Button
                  type="button"
                  variant="outline"
                  size="xs"
                  onClick={() => {
                    setDraftColor(normalizeHexColor(props.value) ?? '#000000');
                    setColorFormat('hex');
                    setMode('add');
                  }}
                >
                  <PlusIcon data-icon="inline-start" />
                  Add
                </Button>
              </div>
              {customPresets.length > 0 ? (
                <div className="grid grid-cols-6 gap-1.5" role="group" aria-label={`${props.label} custom presets`}>
                  {customPresets.map((color) => (
                  <ContextMenu key={color}>
                    <Tooltip>
                      <TooltipTrigger render={<ContextMenuTrigger render={<Button type="button" variant={props.value === color && !props.mixed ? 'outline' : 'ghost'} size="icon" disabled={props.disabled} aria-label={`Use ${color}`} aria-pressed={props.value === color && !props.mixed} data-color-context-menu-trigger onClick={() => chooseColor(color)} />} />}>
                        <ColorSwatch color={color} />
                      </TooltipTrigger>
                      <ColorTooltipContent color={color} />
                    </Tooltip>
                    <ContextMenuContent>
                      <ContextMenuGroup>
                        <ContextMenuItem variant="destructive" onClick={() => updatePresets(colorPresets.filter((preset) => preset !== color))}>
                          <Trash2Icon />
                          Delete preset
                        </ContextMenuItem>
                      </ContextMenuGroup>
                    </ContextMenuContent>
                  </ContextMenu>
                  ))}
                </div>
              ) : null}
            </FieldGroup>
          ) : (
            <FieldGroup className="gap-3">
              <span className="text-xs font-medium">Add custom color</span>
              <HexColorPicker color={draftColor} onChange={setDraftColor} className="w-full!" />
              <ToggleGroup aria-label="Color format" className="grid w-full grid-cols-3" spacing={0} variant="outline" value={[colorFormat]} onValueChange={(next) => next[0] && setColorFormat(next[0] as 'hex' | 'rgb' | 'hsl')}>
                <ToggleGroupItem value="hex">Hex</ToggleGroupItem>
                <ToggleGroupItem value="rgb">RGB</ToggleGroupItem>
                <ToggleGroupItem value="hsl">HSL</ToggleGroupItem>
              </ToggleGroup>
              <ColorFormatFields id={id} format={colorFormat} color={draftColor} onChange={setDraftColor} />
              <div className="grid grid-cols-2 gap-2">
                <Button type="button" variant="outline" size="sm" onClick={() => setMode('presets')}>Cancel</Button>
                <Button type="button" size="sm" disabled={!normalizeHexColor(draftColor)} onClick={addPreset}>Add</Button>
              </div>
            </FieldGroup>
          )}
        </PopoverContent>
      </Popover>
    </PropertyShell>
  );
}

function ColorFormatFields({ id, format, color, onChange }: { id: string; format: 'hex' | 'rgb' | 'hsl'; color: string; onChange: (color: string) => void }) {
  if (format === 'hex') {
    return (
      <Field>
        <FieldLabel htmlFor={`${id}-preset-hex`} className="text-xs">Hex</FieldLabel>
        <InputGroup>
          <InputGroupAddon align="inline-start">
            <Tooltip>
              <TooltipTrigger render={<span className="inline-flex" />}><ColorSwatch color={normalizeHexColor(color) ?? 'transparent'} /></TooltipTrigger>
              <ColorTooltipContent color={normalizeHexColor(color) ?? 'transparent'} />
            </Tooltip>
          </InputGroupAddon>
          <InputGroupInput id={`${id}-preset-hex`} value={color} aria-label="Custom preset hex color" onChange={(event) => onChange(event.target.value)} />
        </InputGroup>
      </Field>
    );
  }

  if (format === 'rgb') {
    const rgb = hexToRgb(color) ?? { r: 0, g: 0, b: 0 };
    return (
      <FieldGroup className="grid grid-cols-3 gap-2">
        {(['r', 'g', 'b'] as const).map((channel) => (
          <Field key={channel}>
            <FieldLabel htmlFor={`${id}-preset-${channel}`} className="text-xs uppercase">{channel}</FieldLabel>
            <Input id={`${id}-preset-${channel}`} type="number" min={0} max={255} value={rgb[channel]} aria-label={`Custom preset ${channel.toUpperCase()}`} onChange={(event) => onChange(rgbToHex({ ...rgb, [channel]: event.target.valueAsNumber }))} />
          </Field>
        ))}
      </FieldGroup>
    );
  }

  const hsl = hexToHsl(color) ?? { h: 0, s: 0, l: 0 };
  const ranges = { h: 360, s: 100, l: 100 } as const;
  return (
    <FieldGroup className="grid grid-cols-3 gap-2">
      {(['h', 's', 'l'] as const).map((channel) => (
        <Field key={channel}>
          <FieldLabel htmlFor={`${id}-preset-${channel}`} className="text-xs uppercase">{channel}</FieldLabel>
          <Input id={`${id}-preset-${channel}`} type="number" min={0} max={ranges[channel]} value={hsl[channel]} aria-label={`Custom preset ${channel.toUpperCase()}`} onChange={(event) => onChange(hslToHex({ ...hsl, [channel]: event.target.valueAsNumber }))} />
        </Field>
      ))}
    </FieldGroup>
  );
}

function ColorTooltipContent({ color }: { color: string }) {
  if (color === 'transparent' || !normalizeHexColor(color)) {
    return <TooltipContent><span>Transparent</span></TooltipContent>;
  }
  const hex = normalizeHexColor(color)!;
  const rgb = hexToRgb(hex)!;
  const hsl = hexToHsl(hex)!;
  return (
    <TooltipContent className="flex-col items-start gap-1 py-2">
      <div className="flex items-center gap-2 font-medium"><ColorSwatch color={hex} surface="tooltip" />{hex.toUpperCase()}</div>
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 font-mono text-[11px]">
        <dt>HEX</dt><dd>{hex.toUpperCase()}</dd>
        <dt>RGB</dt><dd>{rgb.r}, {rgb.g}, {rgb.b}</dd>
        <dt>HSL</dt><dd>{hsl.h}°, {hsl.s}%, {hsl.l}%</dd>
      </dl>
    </TooltipContent>
  );
}

function ColorSwatch({ color, surface = 'panel' }: { color: string; surface?: 'panel' | 'tooltip' }) {
  const normalized = normalizeHexColor(color);
  const needsContrastRing = normalized === '#000000' || normalized === '#ffffff';
  return <span aria-hidden="true" className={cn('size-4 shrink-0 rounded-full border border-border', color === 'transparent' && 'bg-[linear-gradient(135deg,transparent_44%,var(--destructive)_45%,var(--destructive)_55%,transparent_56%)]', needsContrastRing && surface === 'panel' && 'border-transparent ring-1 ring-border', needsContrastRing && surface === 'tooltip' && 'border-transparent ring-1 ring-background/50')} style={color === 'transparent' ? undefined : { backgroundColor: color }} />;
}

export function NumericPropertyField(
  props: ControlledPropertyFieldProps<number> & {
    min?: number;
    max?: number;
    step?: number;
    unit?: string;
    slider?: boolean;
  },
) {
  const id = useId();
  const step = props.step ?? 1;
  const displayValue = normalizeNumberToStep(props.value, step);
  const [editing, setEditing] = useState(false);
  const [draftValue, setDraftValue] = useState(() => String(displayValue));

  useEffect(() => {
    if (!editing) setDraftValue(String(displayValue));
  }, [displayValue, editing]);

  const clamp = (value: number) => Math.min(props.max ?? Number.POSITIVE_INFINITY, Math.max(props.min ?? Number.NEGATIVE_INFINITY, value));
  const update = (value: number, commit = false) => {
    if (!Number.isFinite(value)) return;
    const next = clamp(value);
    props.onChange(next);
    if (commit) props.onCommit?.(next);
  };
  const input = (
    <InputGroup>
      <InputGroupInput
        id={id}
        type="text"
        inputMode="decimal"
        value={props.mixed ? '' : editing ? draftValue : String(displayValue)}
        placeholder={props.mixed ? 'Mixed' : undefined}
        disabled={props.disabled}
        aria-invalid={Boolean(props.validation) || undefined}
        onFocus={() => setEditing(true)}
        onClick={(event) => event.currentTarget.select()}
        onChange={(event) => {
          setDraftValue(event.target.value);
          update(Number.parseFloat(event.target.value));
        }}
        onBlur={(event) => {
          const next = Number.parseFloat(event.currentTarget.value);
          update(next, true);
          setDraftValue(Number.isFinite(next) ? String(normalizeNumberToStep(clamp(next), step)) : String(displayValue));
          setEditing(false);
        }}
      />
      <InputGroupAddon align="inline-end">
        {props.unit ? <InputGroupText>{props.unit}</InputGroupText> : null}
      </InputGroupAddon>
    </InputGroup>
  );
  return (
    <PropertyShell {...props} htmlFor={id} className={props.slider ? 'col-span-2' : undefined}>
      {props.slider && props.min !== undefined && props.max !== undefined ? (
        <div
          className="grid min-h-8 grid-cols-[minmax(0,1fr)_5.5rem] items-center gap-3"
          data-slider-wheel-area
          onWheel={(event) => {
            if (props.disabled) return;
            const wheelDirection = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : -event.deltaY;
            if (wheelDirection === 0) return;
            event.preventDefault();
            event.stopPropagation();
            update(normalizeNumberToStep(props.value + Math.sign(wheelDirection) * step, step), true);
          }}
        >
          <Slider
            aria-label={props.label}
            value={[props.value]}
            min={props.min}
            max={props.max}
            step={step}
            disabled={props.disabled}
            onValueChange={(next) => props.onChange(Array.isArray(next) ? next[0] : (next as number))}
            onValueCommitted={(next) => props.onCommit?.(Array.isArray(next) ? next[0] : (next as number))}
          />
          {input}
        </div>
      ) : (
        input
      )}
    </PropertyShell>
  );
}

function normalizeNumberToStep(value: number, step: number): number {
  const decimalPlaces = Math.min(12, Math.max(0, (step.toString().split('.')[1] ?? '').length));
  const factor = 10 ** decimalPlaces;
  return Math.round(value * factor) / factor;
}

export interface PropertyOption {
  value: string;
  label: string;
  preview?: ReactNode;
}

export function SelectPropertyField(
  props: ControlledPropertyFieldProps<string> & {
    options: readonly PropertyOption[];
  },
) {
  const id = useId();
  const selectedOption = props.options.find((option) => option.value === props.value) ?? null;
  return (
    <PropertyShell {...props} htmlFor={id}>
      <Select
        items={props.options}
        value={props.mixed ? null : selectedOption}
        disabled={props.disabled}
        onValueChange={(next) => {
          if (next) {
            props.onChange(next.value);
            props.onCommit?.(next.value);
          }
        }}
      >
        <SelectTrigger id={id} className="w-full" aria-invalid={Boolean(props.validation) || undefined}>
          <SelectValue>{(value) => value?.label ?? (props.mixed ? 'Mixed' : 'Select')}</SelectValue>
        </SelectTrigger>
        <SelectContent align="start" className="max-h-64 max-w-[min(18rem,var(--available-width))]">
          <SelectGroup>
            {props.options.map((option) => (
              <SelectItem key={option.value} value={option}>
                {option.preview}
                {option.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </PropertyShell>
  );
}

export function SearchableFontPropertyField(props: ControlledPropertyFieldProps<string> & { fonts?: readonly AnnotationFontOption[]; className?: string }) {
  const id = useId();
  const defaultFonts = props.fonts ?? ANNOTATION_FONT_OPTIONS;
  const selectedFont = annotationFontOption(props.value);
  const fonts = defaultFonts.some((font) => font.id === selectedFont.id)
    ? defaultFonts
    : [selectedFont, ...defaultFonts];
  return (
    <PropertyShell {...props} htmlFor={id} className={props.className}>
      <Combobox
        items={fonts}
        itemToStringValue={(font) => font.label}
        value={props.mixed ? null : selectedFont}
        disabled={props.disabled}
        onValueChange={(next) => {
          if (next) {
            props.onChange(next.id);
            props.onCommit?.(next.id);
          }
        }}
      >
        <ComboboxInput id={id} className="w-full" style={props.mixed ? undefined : { fontFamily: selectedFont.cssFamily }} placeholder={props.mixed ? 'Mixed' : 'Search fonts'} aria-invalid={Boolean(props.validation) || undefined} disabled={props.disabled} />
        <ComboboxContent>
          <ComboboxEmpty>No fonts found.</ComboboxEmpty>
          <ComboboxList>
            {(font: AnnotationFontOption) => (
              <ComboboxItem key={font.id} value={font}>
                <span style={{ fontFamily: font.cssFamily }}>{font.label}</span>
              </ComboboxItem>
            )}
          </ComboboxList>
        </ComboboxContent>
      </Combobox>
    </PropertyShell>
  );
}

const COMMON_FONT_SIZES = [6, 8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 60, 72, 96, 120, 144] as const;

export function FontSizePropertyField(props: ControlledPropertyFieldProps<number>) {
  const id = useId();
  const sizes = [...new Set([...COMMON_FONT_SIZES, props.value])]
    .filter((size) => size >= 1 && size <= 144)
    .sort((left, right) => left - right)
    .map((size) => ({ label: `${size} pt`, value: size }));
  const selectedSize = sizes.find((size) => size.value === props.value) ?? null;

  const commitInput = (input: HTMLInputElement) => {
    const next = Number.parseFloat(input.value);
    if (!Number.isFinite(next)) return;
    const clamped = Math.min(144, Math.max(1, next));
    props.onChange(clamped);
    props.onCommit?.(clamped);
  };

  return (
    <PropertyShell {...props} htmlFor={id}>
      <Combobox
        items={sizes}
        value={props.mixed ? null : selectedSize}
        disabled={props.disabled}
        onValueChange={(next) => {
          if (!next) return;
          props.onChange(next.value);
          props.onCommit?.(next.value);
        }}
      >
        <ComboboxInput
          id={id}
          className="w-full"
          placeholder={props.mixed ? 'Mixed' : 'Enter size'}
          inputMode="decimal"
          aria-label="Font size"
          disabled={props.disabled}
          onClick={(event) => event.currentTarget.select()}
          onBlur={(event) => commitInput(event.currentTarget)}
          onKeyDown={(event) => {
            if (event.key !== 'Enter') return;
            commitInput(event.currentTarget);
          }}
        />
        <ComboboxContent>
          <ComboboxEmpty>Enter a size from 1 to 144 pt.</ComboboxEmpty>
          <ComboboxList>
            {(size: { label: string; value: number }) => (
              <ComboboxItem key={size.value} value={size}>{size.label}</ComboboxItem>
            )}
          </ComboboxList>
        </ComboboxContent>
      </Combobox>
    </PropertyShell>
  );
}

export function BooleanPropertyField(props: ControlledPropertyFieldProps<boolean>) {
  const id = useId();
  return (
    <Field orientation="horizontal" data-disabled={props.disabled || undefined} data-invalid={Boolean(props.validation) || undefined}>
      <FieldLabel htmlFor={id} className="text-xs">
        {props.label}
        {props.mixed ? <span className="text-[10px] text-muted-foreground">Mixed</span> : null}
      </FieldLabel>
      <Switch
        id={id}
        checked={props.mixed ? false : props.value}
        disabled={props.disabled}
        onCheckedChange={(next) => {
          props.onChange(next);
          props.onCommit?.(next);
        }}
      />
      <FieldError>{props.validation}</FieldError>
    </Field>
  );
}

export function TogglePropertyField(
  props: ControlledPropertyFieldProps<string> & {
    options: readonly PropertyOption[];
  },
) {
  return (
    <PropertyShell {...props}>
      <ToggleGroup
        value={props.mixed ? [] : [props.value]}
        onValueChange={(next) => {
          const value = next.at(-1);
          if (value) {
            props.onChange(value);
            props.onCommit?.(value);
          }
        }}
        disabled={props.disabled}
        className="w-full"
        spacing={0}
      >
        {props.options.map((option) => (
          <ToggleGroupItem key={option.value} value={option.value} variant="outline" className="flex-1" aria-label={option.label}>
            {option.preview ?? option.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </PropertyShell>
  );
}

const LINE_STYLES: PropertyOption[] = [
  {
    value: 'solid',
    label: 'Solid',
    preview: <span className="w-16 border-t-2 border-current" />,
  },
  {
    value: 'dashed',
    label: 'Dashed',
    preview: <span className="w-16 border-t-2 border-dashed border-current" />,
  },
  {
    value: 'dotted',
    label: 'Dotted',
    preview: <span className="w-16 border-t-2 border-dotted border-current" />,
  },
  {
    value: 'cloud',
    label: 'Cloud',
    preview: <span className="text-xs">☁︎</span>,
  },
];

export function LineStylePropertyField(props: ControlledPropertyFieldProps<string>) {
  return <SelectPropertyField {...props} options={LINE_STYLES} />;
}

export function EndpointPropertyFields({ start, end, scale, filled, onChange }: { start: string; end: string; scale: number; filled: string; onChange: (key: 'start' | 'end' | 'scale' | 'filled', value: string | number) => void }) {
  const options = ['none', 'open-arrow', 'closed-arrow', 'circle', 'square'].map((value) => ({ value, label: value.replaceAll('-', ' ') }));
  return (
    <>
      <SelectPropertyField label="Start" value={start} options={options} onChange={(value) => onChange('start', value)} />
      <SelectPropertyField label="End" value={end} options={options} onChange={(value) => onChange('end', value)} />
      <NumericPropertyField label="Endpoint Scale" value={scale} min={10} max={500} step={5} unit="%" slider onChange={(value) => onChange('scale', value)} />
      <ColorPropertyField label="Endpoint Fill" value={filled} allowTransparent onChange={(value) => onChange('filled', value)} />
    </>
  );
}

export interface TypographyPropertyValue {
  font: string;
  size: number;
  alignment: string;
  verticalAlignment: string;
  styles: string[];
  lineSpacing: number;
  margin: number;
  autoSize: boolean;
}

export function TypographyPropertyFields({ value, color, onChange, onColorChange }: { value: TypographyPropertyValue; color?: string; onChange: (value: TypographyPropertyValue) => void; onColorChange?: (color: string) => void }) {
  const update = <K extends keyof TypographyPropertyValue>(key: K, next: TypographyPropertyValue[K]) => onChange({ ...value, [key]: next });
  const icons: Record<string, ReactNode> = {
    left: <AlignLeftIcon />,
    center: <AlignCenterIcon />,
    right: <AlignRightIcon />,
  };
  return (
    <>
      <PropertySection title="Text">
        <SearchableFontPropertyField label="Font" value={value.font} className="col-span-2" onChange={(next) => update('font', next)} />
        {color !== undefined && onColorChange ? <ColorPropertyField label="Color" value={color} onChange={onColorChange} /> : null}
        <FontSizePropertyField label="Font Size" value={value.size} onChange={(next) => update('size', next)} />
        <PropertyShell label="Font Styles">
          <div className="flex flex-col gap-2">
            <ToggleGroup multiple aria-label="Font emphasis" className="grid w-full grid-cols-4" spacing={0} variant="outline" value={value.styles.filter((style) => ['bold', 'italic', 'underline', 'strikethrough'].includes(style))} onValueChange={(next) => update('styles', [...value.styles.filter((style) => !['bold', 'italic', 'underline', 'strikethrough'].includes(style)), ...next])}>
              <ToggleGroupItem className="w-full" value="bold" aria-label="Bold">
                <BoldIcon />
              </ToggleGroupItem>
              <ToggleGroupItem className="w-full" value="italic" aria-label="Italic">
                <ItalicIcon />
              </ToggleGroupItem>
              <ToggleGroupItem className="w-full" value="underline" aria-label="Underline">
                <UnderlineIcon />
              </ToggleGroupItem>
              <ToggleGroupItem className="w-full" value="strikethrough" aria-label="Strikethrough">
                <StrikethroughIcon />
              </ToggleGroupItem>
            </ToggleGroup>
            <ToggleGroup
              aria-label="Font script"
              className="grid w-full grid-cols-3"
              spacing={0}
              variant="outline"
              value={[value.styles.find((style) => style === 'superscript' || style === 'subscript') ?? 'normal']}
              onValueChange={(next) => {
                const script = next[0] ?? 'normal';
                update('styles', [...value.styles.filter((style) => style !== 'superscript' && style !== 'subscript'), ...(script === 'normal' ? [] : [script])]);
              }}
            >
              <ToggleGroupItem className="w-full" value="normal">
                Normal
              </ToggleGroupItem>
              <ToggleGroupItem className="w-full" value="superscript" aria-label="Superscript">
                x²
              </ToggleGroupItem>
              <ToggleGroupItem className="w-full" value="subscript" aria-label="Subscript">
                x₂
              </ToggleGroupItem>
            </ToggleGroup>
          </div>
        </PropertyShell>
      </PropertySection>
      <PropertySection title="Alignment">
        <TogglePropertyField
          label="Horizontal"
          value={value.alignment}
          options={['left', 'center', 'right'].map((key) => ({
            value: key,
            label: key,
            preview: icons[key],
          }))}
          onChange={(next) => update('alignment', next)}
        />
        <TogglePropertyField
          label="Vertical"
          value={value.verticalAlignment}
          options={['top', 'middle', 'bottom'].map((key) => ({
            value: key,
            label: key,
          }))}
          onChange={(next) => update('verticalAlignment', next)}
        />
        <NumericPropertyField label="Line Spacing" value={value.lineSpacing} min={0.5} max={3} step={0.1} unit="×" onChange={(next) => update('lineSpacing', next)} />
      </PropertySection>
      <PropertySection title="Text Box">
        <NumericPropertyField label="Margin" value={value.margin} min={0} max={72} step={0.5} unit="pt" onChange={(next) => update('margin', next)} />
        <BooleanPropertyField label="Automatic Sizing" value={value.autoSize} onChange={(next) => update('autoSize', next)} />
      </PropertySection>
    </>
  );
}

export function CommentsPropertyField(props: ControlledPropertyFieldProps<string>) {
  const id = useId();
  return (
    <PropertyShell {...props} htmlFor={id}>
      <Textarea id={id} value={props.mixed ? '' : props.value} placeholder={props.mixed ? 'Mixed' : 'Add a comment'} disabled={props.disabled} onChange={(event) => props.onChange(event.target.value)} onBlur={() => props.onCommit?.(props.value)} />
    </PropertyShell>
  );
}

export function SimpleTextPropertyField(props: ControlledPropertyFieldProps<string>) {
  const id = useId();
  return (
    <PropertyShell {...props} htmlFor={id}>
      <Input id={id} value={props.mixed ? '' : props.value} placeholder={props.mixed ? 'Mixed' : undefined} disabled={props.disabled} onChange={(event) => props.onChange(event.target.value)} onBlur={() => props.onCommit?.(props.value)} />
    </PropertyShell>
  );
}
