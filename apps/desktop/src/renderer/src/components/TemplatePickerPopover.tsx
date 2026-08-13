import { CheckIcon, ChevronDown, Settings2Icon } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Item, ItemContent, ItemDescription, ItemGroup, ItemTitle } from '@/components/ui/item';
import { Popover, PopoverContent, PopoverHeader, PopoverTitle, PopoverTrigger } from '@/components/ui/popover';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { SplitButtonSegment } from './domain-ui/SplitButtonSegment';
import { TemplatePreviewCard } from './TemplatePreviewCard';
import { allTemplates, templateGridSummary, templateSummary, type PdfTemplate } from './templateLibrary';
import type { ReturnTypeOfLoadTemplateLibrary } from './templateLibraryTypes';

interface TemplatePickerPopoverProps {
  library: ReturnTypeOfLoadTemplateLibrary;
  tooltipSide?: 'top' | 'bottom';
  onCreate: (template: PdfTemplate) => void | Promise<void>;
  onManage: () => void;
  onUseTemplate: (templateId: string) => void;
}

export function TemplatePickerPopover({ library, tooltipSide, onCreate, onManage, onUseTemplate }: TemplatePickerPopoverProps) {
  const templates = allTemplates(library);
  const initialTemplate = templates.find((template) => template.id === library.lastTemplateId) ?? templates[0];
  const [open, setOpen] = useState(false);
  const [selectedId, setSelectedId] = useState(initialTemplate.id);
  const [creating, setCreating] = useState(false);
  const selected = templates.find((template) => template.id === selectedId) ?? initialTemplate;

  useEffect(() => {
    if (open) setSelectedId(library.lastTemplateId);
  }, [library.lastTemplateId, open]);

  async function createSelected(): Promise<void> {
    setCreating(true);
    try {
      await Promise.resolve(onCreate(selected));
      onUseTemplate(selected.id);
      setOpen(false);
    } finally {
      setCreating(false);
    }
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <Tooltip disabled={open}>
        <PopoverTrigger render={(
          <TooltipTrigger render={(
            <SplitButtonSegment
              type="button"
              size="icon"
              aria-label="New from template"
              data-testid="document-tab-template-picker"
            >
              <ChevronDown data-icon="inline-start" aria-hidden="true" />
            </SplitButtonSegment>
          )} />
        )} />
        <TooltipContent side={tooltipSide}>New from template…</TooltipContent>
      </Tooltip>
      <PopoverContent
        align="start"
        className="flex w-[560px] max-w-[calc(100vw-16px)] flex-col gap-3"
        data-testid="template-picker"
        finalFocus={() => document.querySelector<HTMLElement>('[data-testid="document-tab-template-picker"]')}
      >
        <PopoverHeader>
          <PopoverTitle>New from template</PopoverTitle>
        </PopoverHeader>
        <div className="grid min-h-0 grid-cols-[minmax(0,1fr)_240px] gap-3">
          <ScrollArea className="h-72">
            <ItemGroup className="gap-1 pr-3" data-testid="template-picker-list">
              {templates.map((template) => (
                <Item
                  key={template.id}
                  render={<Button type="button" variant="ghost" />}
                  variant={selected.id === template.id ? 'muted' : 'default'}
                  size="sm"
                  className="flex-nowrap text-left"
                  data-testid={`template-picker-item-${template.id}`}
                  onClick={() => setSelectedId(template.id)}
                  onDoubleClick={() => {
                    setSelectedId(template.id);
                    void Promise.resolve(onCreate(template)).then(() => {
                      onUseTemplate(template.id);
                      setOpen(false);
                    });
                  }}
                >
                  <ItemContent>
                    <ItemTitle>{template.name}</ItemTitle>
                    <ItemDescription>{templateSummary(template)} · {templateGridSummary(template)}</ItemDescription>
                  </ItemContent>
                  {selected.id === template.id ? <CheckIcon aria-hidden="true" /> : null}
                </Item>
              ))}
            </ItemGroup>
          </ScrollArea>
          <TemplatePreviewCard template={selected} compact />
        </div>
        <div className="flex items-center justify-between gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              setOpen(false);
              window.setTimeout(onManage, 150);
            }}
          >
            <Settings2Icon data-icon="inline-start" aria-hidden="true" />
            Manage templates…
          </Button>
          <Button type="button" disabled={creating} data-testid="template-picker-create" onClick={() => void createSelected()}>
            {creating ? 'Creating…' : 'Create'}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
