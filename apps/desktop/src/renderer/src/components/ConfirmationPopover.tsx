import type { ReactElement, ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover';

interface ConfirmationPopoverProps {
  actionLabel: string;
  actionVariant?: 'default' | 'destructive';
  align?: 'start' | 'center' | 'end';
  busy?: boolean;
  description: ReactNode;
  onAction: () => void;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  secondaryActionLabel?: string;
  secondaryActionVariant?: 'default' | 'destructive' | 'outline';
  onSecondaryAction?: () => void;
  side?: 'top' | 'bottom' | 'left' | 'right' | 'inline-start' | 'inline-end';
  title: ReactNode;
  trigger: ReactElement;
}

/**
 * A local confirmation anchored to the control that requested it. The popup
 * keeps the source and scope visible instead of adding a page-wide backdrop.
 */
export function ConfirmationPopover({
  actionLabel,
  actionVariant = 'default',
  align = 'center',
  busy = false,
  description,
  onAction,
  onOpenChange,
  open,
  secondaryActionLabel,
  secondaryActionVariant = 'outline',
  onSecondaryAction,
  side = 'bottom',
  title,
  trigger,
}: ConfirmationPopoverProps) {
  return (
    <Popover open={open} onOpenChange={(nextOpen) => { if (!busy) onOpenChange(nextOpen); }}>
      <PopoverTrigger render={trigger} />
      <PopoverContent
        align={align}
        side={side}
        sideOffset={8}
        className="w-80 gap-3 p-3"
        data-testid="confirmation-popover"
      >
        <PopoverHeader>
          <PopoverTitle>{title}</PopoverTitle>
          <PopoverDescription>{description}</PopoverDescription>
        </PopoverHeader>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="outline" disabled={busy} onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          {secondaryActionLabel && onSecondaryAction ? (
            <Button type="button" variant={secondaryActionVariant} disabled={busy} onClick={onSecondaryAction}>
              {secondaryActionLabel}
            </Button>
          ) : null}
          <Button type="button" variant={actionVariant} disabled={busy} onClick={onAction}>
            {actionLabel}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
