import type { ReactElement } from 'react';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { TemplatePreviewCard } from '../TemplatePreviewCard';
import type { PdfTemplate } from '../templateLibrary';

/**
 * Domain UI exception: the official tooltip has one compact foreground-colour
 * treatment. The last-template preview needs a larger non-interactive popover
 * surface, so its popup and the compound tooltip arrow must use the same
 * semantic surface colour.
 */
export function LastTemplatePreviewTooltip({ template, side, trigger }: {
  template: PdfTemplate;
  side?: 'top' | 'bottom';
  trigger: ReactElement;
}) {
  return (
    <Tooltip>
      <TooltipTrigger render={trigger} />
      <TooltipContent
        side={side}
        className="block w-72 max-w-[calc(100vw-16px)] bg-popover text-popover-foreground shadow-md [&>div[aria-hidden=true]:last-child]:bg-popover!"
        data-domain-ui-exception="last-template-preview-tooltip"
        data-testid="document-tab-last-template-preview"
      >
        <TemplatePreviewCard template={template} compact />
        <p className="mt-2 text-xs text-muted-foreground">Click to create</p>
      </TooltipContent>
    </Tooltip>
  );
}
