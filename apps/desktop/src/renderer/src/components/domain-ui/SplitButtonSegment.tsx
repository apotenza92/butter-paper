import type { ComponentProps } from 'react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

type SplitButtonSegmentProps = Omit<ComponentProps<typeof Button>, 'variant'> & {
  selected?: boolean;
};

const RESTING_SURFACE = 'bg-transparent! hover:bg-muted! aria-expanded:bg-transparent! dark:bg-transparent! dark:hover:bg-muted/50! dark:aria-expanded:bg-transparent!';
const SELECTED_SURFACE = 'bg-muted! hover:bg-muted! aria-expanded:bg-muted! dark:bg-muted! dark:hover:bg-muted! dark:aria-expanded:bg-muted!';

export function resolveSplitButtonSegmentSurface(selected: boolean): string {
  return selected ? SELECTED_SURFACE : RESTING_SURFACE;
}

/**
 * Domain UI exception: shadcn ButtonGroup does not synchronize the dark-mode
 * surface of an outline Button with an adjacent Toggle or with a related ghost
 * action. This segment keeps the official Button behavior and geometry while
 * making its resting surface follow the split control's product state instead
 * of its popup-open state.
 */
export function SplitButtonSegment({
  className,
  selected = false,
  ...props
}: SplitButtonSegmentProps) {
  return (
    <Button
      data-domain-ui-exception="split-button-segment"
      variant="outline"
      className={cn(
        resolveSplitButtonSegmentSurface(selected),
        className,
      )}
      {...props}
    />
  );
}
