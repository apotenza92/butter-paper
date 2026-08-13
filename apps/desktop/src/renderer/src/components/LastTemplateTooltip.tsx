import type { ReactElement } from 'react';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { TemplatePreviewCard } from './TemplatePreviewCard';
import type { PdfTemplate } from './templateLibrary';

export function LastTemplateTooltip({ template, side, trigger }: {
  template: PdfTemplate;
  side?: 'top' | 'bottom';
  trigger: ReactElement;
}) {
  return (
    <Tooltip>
      <TooltipTrigger render={trigger} />
      <TooltipContent
        side={side}
        className="block w-72 max-w-[calc(100vw-16px)] bg-popover text-popover-foreground shadow-md"
        data-testid="document-tab-last-template-preview"
      >
        <TemplatePreviewCard template={template} compact />
        <p className="mt-2 text-xs text-muted-foreground">Click to create</p>
      </TooltipContent>
    </Tooltip>
  );
}
