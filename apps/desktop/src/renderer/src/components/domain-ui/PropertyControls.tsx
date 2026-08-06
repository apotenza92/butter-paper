import { useId, type ReactNode } from 'react';
import { AlignCenterIcon, AlignLeftIcon, AlignRightIcon, BoldIcon, ItalicIcon, MinusIcon, PlusIcon, StrikethroughIcon, UnderlineIcon } from 'lucide-react';
import { Combobox, ComboboxContent, ComboboxEmpty, ComboboxInput, ComboboxItem, ComboboxList } from '@/components/ui/combobox';
import { Button } from '@/components/ui/button';
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel, FieldLegend, FieldSet } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput, InputGroupText } from '@/components/ui/input-group';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Slider } from '@/components/ui/slider';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { cn } from '@/lib/utils';

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
  children: ReactNode;
}

function PropertyShell({ label, mixed, disabled, description, validation, htmlFor, children }: PropertyShellProps) {
  return (
    <Field data-disabled={disabled || undefined} data-invalid={Boolean(validation) || undefined}>
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

export function PropertySection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <FieldSet className="gap-3" data-domain-ui-exception="property-controls">
      <FieldLegend variant="label" className="mb-0 text-xs">
        {title}
      </FieldLegend>
      <FieldGroup className="gap-3">{children}</FieldGroup>
    </FieldSet>
  );
}

const COLOR_PRESETS = ['#000000', '#ef4444', '#f59e0b', '#facc15', '#22c55e', '#3b82f6', '#8b5cf6', '#ffffff'];

export function ColorPropertyField(props: ControlledPropertyFieldProps<string> & { allowTransparent?: boolean }) {
  const id = useId();
  const presets = props.allowTransparent ? ['transparent', ...COLOR_PRESETS] : COLOR_PRESETS;
  const displayValue = props.mixed ? 'Mixed' : props.value === 'transparent' ? 'Transparent' : props.value.toUpperCase();
  return (
    <PropertyShell {...props} htmlFor={id}>
      <Popover>
        <PopoverTrigger render={<Button id={id} type="button" variant="outline" className="w-full justify-start" disabled={props.disabled} aria-invalid={Boolean(props.validation) || undefined} />}>
          <ColorSwatch color={props.mixed ? 'transparent' : props.value} />
          <span className="truncate">{displayValue}</span>
        </PopoverTrigger>
        <PopoverContent align="start" className="w-[min(16rem,var(--available-width))]">
          <FieldGroup className="gap-3">
            <div className="grid grid-cols-5 gap-1.5" role="group" aria-label={`${props.label} presets`}>
              {presets.map((color) => (
                <Button
                  key={color}
                  type="button"
                  variant={props.value === color && !props.mixed ? 'outline' : 'ghost'}
                  size="icon-sm"
                  disabled={props.disabled}
                  aria-label={color === 'transparent' ? 'Transparent' : `Use ${color}`}
                  aria-pressed={props.value === color && !props.mixed}
                  onClick={() => {
                    props.onChange(color);
                    props.onCommit?.(color);
                  }}
                >
                  <ColorSwatch color={color} />
                </Button>
              ))}
            </div>
            <InputGroup>
              <InputGroupInput id={`${id}-custom`} value={props.mixed ? '' : props.value} placeholder={props.mixed ? 'Mixed' : '#000000'} disabled={props.disabled} aria-label={`Custom ${props.label.toLowerCase()}`} aria-invalid={Boolean(props.validation) || undefined} onChange={(event) => props.onChange(event.target.value)} onBlur={() => props.onCommit?.(props.value)} />
              <InputGroupAddon align="inline-start">
                <ColorSwatch color={props.mixed ? 'transparent' : props.value} />
              </InputGroupAddon>
            </InputGroup>
          </FieldGroup>
        </PopoverContent>
      </Popover>
    </PropertyShell>
  );
}

function ColorSwatch({ color }: { color: string }) {
  return <span aria-hidden="true" className={cn('size-4 shrink-0 rounded-full border border-border', color === 'transparent' && 'bg-[linear-gradient(135deg,transparent_44%,var(--destructive)_45%,var(--destructive)_55%,transparent_56%)]')} style={color === 'transparent' ? undefined : { backgroundColor: color }} />;
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
  const clamp = (value: number) => Math.min(props.max ?? Number.POSITIVE_INFINITY, Math.max(props.min ?? Number.NEGATIVE_INFINITY, value));
  const update = (value: number, commit = false) => {
    if (!Number.isFinite(value)) return;
    const next = clamp(value);
    props.onChange(next);
    if (commit) props.onCommit?.(next);
  };
  const input = (
    <InputGroup>
      <InputGroupInput id={id} type="number" value={props.mixed ? '' : props.value} placeholder={props.mixed ? 'Mixed' : undefined} min={props.min} max={props.max} step={step} disabled={props.disabled} aria-invalid={Boolean(props.validation) || undefined} onChange={(event) => update(event.target.valueAsNumber)} onBlur={(event) => update(event.currentTarget.valueAsNumber, true)} />
      {!props.slider ? (
        <InputGroupAddon align="inline-start">
          <InputGroupButton size="icon-xs" aria-label={`Decrease ${props.label}`} disabled={props.disabled} onClick={() => update(props.value - step, true)}>
            <MinusIcon />
          </InputGroupButton>
        </InputGroupAddon>
      ) : null}
      <InputGroupAddon align="inline-end">
        {props.unit ? <InputGroupText>{props.unit}</InputGroupText> : null}
        {!props.slider ? (
          <InputGroupButton size="icon-xs" aria-label={`Increase ${props.label}`} disabled={props.disabled} onClick={() => update(props.value + step, true)}>
            <PlusIcon />
          </InputGroupButton>
        ) : null}
      </InputGroupAddon>
    </InputGroup>
  );
  return (
    <PropertyShell {...props} htmlFor={id}>
      {props.slider && props.min !== undefined && props.max !== undefined ? (
        <div className="grid grid-cols-[minmax(0,1fr)_5.5rem] items-center gap-3">
          <Slider aria-label={props.label} value={[props.value]} min={props.min} max={props.max} step={step} disabled={props.disabled} onValueChange={(next) => props.onChange(Array.isArray(next) ? next[0] : (next as number))} onValueCommitted={(next) => props.onCommit?.(Array.isArray(next) ? next[0] : (next as number))} />
          {input}
        </div>
      ) : (
        input
      )}
    </PropertyShell>
  );
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

export function SearchableFontPropertyField(props: ControlledPropertyFieldProps<string> & { fonts: readonly string[] }) {
  const id = useId();
  const fonts = props.fonts.map((font) => ({ label: font, value: font }));
  const selectedFont = fonts.find((font) => font.value === props.value) ?? null;
  return (
    <PropertyShell {...props} htmlFor={id}>
      <Combobox
        items={fonts}
        value={props.mixed ? null : selectedFont}
        disabled={props.disabled}
        onValueChange={(next) => {
          if (next) {
            props.onChange(next.value);
            props.onCommit?.(next.value);
          }
        }}
      >
        <ComboboxInput id={id} className="w-full" placeholder={props.mixed ? 'Mixed' : 'Search fonts'} aria-invalid={Boolean(props.validation) || undefined} disabled={props.disabled} />
        <ComboboxContent>
          <ComboboxEmpty>No fonts found.</ComboboxEmpty>
          <ComboboxList>
            {(font: { label: string; value: string }) => (
              <ComboboxItem key={font.value} value={font}>
                <span style={{ fontFamily: font.value }}>{font.label}</span>
              </ComboboxItem>
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

export function TypographyPropertyFields({ value, onChange }: { value: TypographyPropertyValue; onChange: (value: TypographyPropertyValue) => void }) {
  const update = <K extends keyof TypographyPropertyValue>(key: K, next: TypographyPropertyValue[K]) => onChange({ ...value, [key]: next });
  const icons: Record<string, ReactNode> = {
    left: <AlignLeftIcon />,
    center: <AlignCenterIcon />,
    right: <AlignRightIcon />,
  };
  return (
    <>
      <SearchableFontPropertyField label="Font" value={value.font} fonts={['Arial', 'Calibri', 'Courier New', 'Georgia', 'Helvetica', 'Times New Roman', 'Verdana']} onChange={(next) => update('font', next)} />
      <NumericPropertyField label="Font Size" value={value.size} min={1} max={144} unit="pt" onChange={(next) => update('size', next)} />
      <TogglePropertyField
        label="Alignment"
        value={value.alignment}
        options={['left', 'center', 'right'].map((key) => ({
          value: key,
          label: key,
          preview: icons[key],
        }))}
        onChange={(next) => update('alignment', next)}
      />
      <TogglePropertyField
        label="Vertical Alignment"
        value={value.verticalAlignment}
        options={['top', 'middle', 'bottom'].map((key) => ({
          value: key,
          label: key,
        }))}
        onChange={(next) => update('verticalAlignment', next)}
      />
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
      <NumericPropertyField label="Line Spacing" value={value.lineSpacing} min={0.5} max={3} step={0.1} unit="×" onChange={(next) => update('lineSpacing', next)} />
      <NumericPropertyField label="Margin" value={value.margin} min={0} max={72} step={0.5} unit="pt" onChange={(next) => update('margin', next)} />
      <BooleanPropertyField label="Automatic Sizing" value={value.autoSize} onChange={(next) => update('autoSize', next)} />
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
