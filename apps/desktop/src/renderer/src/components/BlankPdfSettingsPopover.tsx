import { ChevronDown } from 'lucide-react';
import { useEffect, useState } from 'react';
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { resolveBlankPdfDimensions, type BlankPdfSettings } from './blankPdfSettings';
import { BlankPdfSettingsFields } from './BlankPdfSettingsFields';
import { SplitButtonSegment } from './domain-ui/SplitButtonSegment';

interface BlankPdfSettingsPopoverProps {
  settings: BlankPdfSettings;
  onSettingsChange: (settings: BlankPdfSettings) => void;
}

export function BlankPdfSettingsPopover({ settings, onSettingsChange }: BlankPdfSettingsPopoverProps) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(settings);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) setDraft(settings);
  }, [open, settings]);

  function handleOpenChange(nextOpen: boolean): void {
    if (nextOpen) {
      setDraft(settings);
      setError(null);
    }
    setOpen(nextOpen);
  }

  function updateSettings(nextSettings: BlankPdfSettings): void {
    setDraft(nextSettings);
    try {
      resolveBlankPdfDimensions(nextSettings);
      onSettingsChange(nextSettings);
      setError(null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to save the blank PDF default.');
    }
  }

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      <Tooltip disabled={open}>
        <PopoverTrigger
          render={(
            <TooltipTrigger
              render={(
                <SplitButtonSegment
                  type="button"
                  size="icon"
                  aria-label="Blank PDF settings"
                  data-testid="document-tab-new-pdf-settings"
                >
                  <ChevronDown data-icon="inline-start" aria-hidden="true" />
                </SplitButtonSegment>
              )}
            />
          )}
        />
        <TooltipContent>Blank PDF settings</TooltipContent>
      </Tooltip>
      <PopoverContent
        align="start"
        className="max-h-[var(--available-height)] w-[320px] overflow-y-auto overscroll-contain"
        data-testid="new-blank-pdf-settings"
        finalFocus={() => document.querySelector<HTMLElement>('[data-testid="document-tab-new-pdf-settings"]')}
      >
        <PopoverHeader>
          <PopoverTitle>Blank PDF default</PopoverTitle>
          <PopoverDescription>Choose the settings used for new PDFs. Changes are saved automatically.</PopoverDescription>
        </PopoverHeader>
        <BlankPdfSettingsFields
          settings={draft}
          error={error}
          testIdPrefix="new-blank-pdf"
          onSettingsChange={updateSettings}
        />
      </PopoverContent>
    </Popover>
  );
}
