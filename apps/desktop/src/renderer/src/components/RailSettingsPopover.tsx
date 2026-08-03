import { ChevronUp, Settings } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Field, FieldLabel } from '@/components/ui/field';
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover';
import { cn } from '@/lib/utils';
import type { RailSide } from './railSettings';

interface RailSettingsPopoverProps {
  side: RailSide;
  expanded: boolean;
  open: boolean;
  expandOnHover: boolean;
  onOpenChange: (open: boolean) => void;
  onExpandOnHoverChange: (enabled: boolean) => void;
}

export function RailSettingsPopover({
  side,
  expanded,
  open,
  expandOnHover,
  onOpenChange,
  onExpandOnHoverChange,
}: RailSettingsPopoverProps) {
  const title = `${side === 'left' ? 'Left' : 'Right'} rail settings`;
  const checkboxId = `${side}-rail-expand-on-hover`;

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger
        render={(
          <Button
            type="button"
            variant="ghost"
            size={expanded ? 'default' : 'icon'}
            className={cn(expanded ? 'w-full justify-start' : 'gap-0')}
            aria-label={title}
            data-testid={`${side}-rail-settings-trigger`}
          >
            <Settings data-icon="inline-start" aria-hidden="true" />
            {expanded ? <span className="truncate">Rail settings</span> : null}
            <ChevronUp data-icon="inline-end" className={cn(expanded && 'ml-auto')} aria-hidden="true" />
          </Button>
        )}
      />
      <PopoverContent
        side="top"
        align={side === 'left' ? 'start' : 'end'}
        sideOffset={8}
        data-testid={`${side}-rail-settings-popover`}
      >
        <PopoverHeader>
          <PopoverTitle>{title}</PopoverTitle>
          <PopoverDescription>
            Show tool names when this rail is one column wide.
          </PopoverDescription>
        </PopoverHeader>
        <Field orientation="horizontal">
          <Checkbox
            id={checkboxId}
            checked={expandOnHover}
            data-testid={`${side}-rail-expand-on-hover`}
            onCheckedChange={onExpandOnHoverChange}
          />
          <FieldLabel htmlFor={checkboxId}>Expand labels on hover</FieldLabel>
        </Field>
      </PopoverContent>
    </Popover>
  );
}
