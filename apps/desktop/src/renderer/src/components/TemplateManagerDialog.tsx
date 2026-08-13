import { FileInputIcon, PlusIcon, Trash2Icon } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { Item, ItemActions, ItemContent, ItemDescription, ItemGroup, ItemTitle } from '@/components/ui/item';
import { ScrollArea } from '@/components/ui/scroll-area';
import { BlankPdfSettingsFields } from './BlankPdfSettingsFields';
import { TemplatePreviewCard } from './TemplatePreviewCard';
import {
  addGeneratedTemplate,
  allTemplates,
  removeTemplate,
  templateGridSummary,
  templateSummary,
  type PdfTemplate,
} from './templateLibrary';
import type { ReturnTypeOfLoadTemplateLibrary } from './templateLibraryTypes';
import { DEFAULT_BLANK_PDF_SETTINGS, resolveBlankPdfDimensions, type BlankPdfSettings } from './blankPdfSettings';

interface TemplateManagerDialogProps {
  open: boolean;
  library: ReturnTypeOfLoadTemplateLibrary;
  onLibraryChange: (library: ReturnTypeOfLoadTemplateLibrary) => void;
  onOpenChange: (open: boolean) => void;
  onImportPdf: () => Promise<void>;
}

export function TemplateManagerDialog({ open, library, onLibraryChange, onOpenChange, onImportPdf }: TemplateManagerDialogProps) {
  const templates = allTemplates(library);
  const [selectedId, setSelectedId] = useState(library.lastTemplateId);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('');
  const [settings, setSettings] = useState<BlankPdfSettings>({ ...DEFAULT_BLANK_PDF_SETTINGS });
  const [error, setError] = useState<string | null>(null);
  const selected = templates.find((template) => template.id === selectedId) ?? templates[0];

  useEffect(() => {
    if (open) {
      setSelectedId(library.lastTemplateId);
      setCreating(false);
      setError(null);
    }
  }, [library.lastTemplateId, open]);

  function beginCreate(): void {
    setCreating(true);
    setName('');
    setSettings({ ...DEFAULT_BLANK_PDF_SETTINGS });
    setError(null);
  }

  function saveCreatedTemplate(): void {
    try {
      resolveBlankPdfDimensions(settings);
      const next = addGeneratedTemplate(library, name, settings);
      onLibraryChange(next);
      setSelectedId(next.lastTemplateId);
      setCreating(false);
      setError(null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to save the template.');
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[calc(100vh-32px)] flex-col sm:max-w-4xl" data-testid="template-manager-dialog">
        <DialogHeader>
          <DialogTitle>Template library</DialogTitle>
          <DialogDescription>Create reusable paper or import a PDF that you already use as a template.</DialogDescription>
        </DialogHeader>
        {creating ? (
          <div className="grid min-h-0 grid-cols-[minmax(0,1fr)_280px] gap-4 overflow-y-auto">
            <FieldGroup>
              <Field data-invalid={Boolean(error)}>
                <FieldLabel htmlFor="template-name">Template name</FieldLabel>
                <Input id="template-name" value={name} aria-invalid={Boolean(error)} autoFocus onChange={(event) => setName(event.currentTarget.value)} />
                {error ? <FieldError>{error}</FieldError> : null}
              </Field>
              <BlankPdfSettingsFields settings={settings} error={null} testIdPrefix="template-manager-create" onSettingsChange={setSettings} />
            </FieldGroup>
            <TemplatePreviewCard template={{ id: 'draft', name: name.trim() || 'New paper template', kind: 'generated', builtIn: false, settings }} />
          </div>
        ) : (
          <div className="grid min-h-0 grid-cols-[minmax(0,1fr)_280px] gap-4">
            <ScrollArea className="h-[430px]">
              <ItemGroup className="gap-1 pr-3">
                {templates.map((template) => (
                  <TemplateLibraryItem
                    key={template.id}
                    template={template}
                    selected={selected.id === template.id}
                    onSelect={() => setSelectedId(template.id)}
                    onRemove={template.builtIn ? undefined : () => {
                      const next = removeTemplate(library, template.id);
                      onLibraryChange(next);
                      setSelectedId(next.lastTemplateId);
                    }}
                  />
                ))}
              </ItemGroup>
            </ScrollArea>
            <TemplatePreviewCard template={selected} />
          </div>
        )}
        <DialogFooter className="sm:justify-between">
          {creating ? (
            <>
              <Button type="button" variant="outline" onClick={() => setCreating(false)}>Cancel</Button>
              <Button type="button" data-testid="template-manager-save" onClick={saveCreatedTemplate}>Save template</Button>
            </>
          ) : (
            <>
              <div className="flex items-center gap-2">
                <Button type="button" onClick={beginCreate}>
                  <PlusIcon data-icon="inline-start" aria-hidden="true" />
                  Create paper template…
                </Button>
                <Button type="button" variant="outline" onClick={() => void onImportPdf()}>
                  <FileInputIcon data-icon="inline-start" aria-hidden="true" />
                  Import PDF as template…
                </Button>
              </div>
              <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>Done</Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function TemplateLibraryItem({ template, selected, onSelect, onRemove }: {
  template: PdfTemplate;
  selected: boolean;
  onSelect: () => void;
  onRemove?: () => void;
}) {
  return (
    <Item variant={selected ? 'muted' : 'default'} size="sm" className="relative flex-nowrap">
      <Button type="button" variant="ghost" className="absolute inset-0 size-full" aria-label={`Select ${template.name}`} onClick={onSelect} />
      <ItemContent className="pointer-events-none min-w-0">
        <ItemTitle>{template.name}</ItemTitle>
        <ItemDescription>{templateSummary(template)} · {templateGridSummary(template)}</ItemDescription>
      </ItemContent>
      {onRemove ? (
        <ItemActions className="relative">
          <Button type="button" variant="ghost" size="icon-sm" aria-label={`Remove ${template.name}`} onClick={onRemove}>
            <Trash2Icon aria-hidden="true" />
          </Button>
        </ItemActions>
      ) : null}
    </Item>
  );
}
