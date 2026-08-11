import { ChevronDown } from 'lucide-react';
import { useEffect, useState } from 'react';
import {
  Popover,
  PopoverContent,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { resolveBlankPdfDimensions, type BlankPdfSettings } from './blankPdfSettings';
import { BlankPdfSettingsFields } from './BlankPdfSettingsFields';
import { BlankPdfPagePreview } from './domain-ui/BlankPdfPagePreview';
import { SplitButtonSegment } from './domain-ui/SplitButtonSegment';

interface BlankPdfSettingsPopoverProps {
  settings: BlankPdfSettings;
  tooltipSide?: 'top' | 'bottom';
  onSettingsChange: (settings: BlankPdfSettings) => void;
}

export function BlankPdfSettingsPopover({ settings, tooltipSide, onSettingsChange }: BlankPdfSettingsPopoverProps) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(settings);
  const [error, setError] = useState<string | null>(null);
  const [viewportHeight, setViewportHeight] = useState(() => window.innerHeight);

  useEffect(() => {
    if (!open) setDraft(settings);
  }, [open, settings]);

  useEffect(() => {
    const updateViewportHeight = () => setViewportHeight(window.innerHeight);
    window.addEventListener('resize', updateViewportHeight);
    window.visualViewport?.addEventListener('resize', updateViewportHeight);
    return () => {
      window.removeEventListener('resize', updateViewportHeight);
      window.visualViewport?.removeEventListener('resize', updateViewportHeight);
    };
  }, []);

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
        <TooltipContent side={tooltipSide}>Blank PDF settings</TooltipContent>
      </Tooltip>
      <PopoverContent
        align="start"
        className="w-[320px] overflow-y-auto overscroll-contain"
        data-testid="new-blank-pdf-settings"
        finalFocus={() => document.querySelector<HTMLElement>('[data-testid="document-tab-new-pdf-settings"]')}
        style={{ maxHeight: Math.max(0, viewportHeight - 16) }}
      >
        <PopoverHeader className="gap-2">
          <PopoverTitle>Blank PDF default</PopoverTitle>
          <BlankPdfPagePreview settings={draft} />
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
