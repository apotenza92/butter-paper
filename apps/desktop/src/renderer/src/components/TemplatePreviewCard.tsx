import { Badge } from '@/components/ui/badge';
import { BlankPdfPagePreview } from './domain-ui/BlankPdfPagePreview';
import { templateGridSummary, templateSummary, type PdfTemplate } from './templateLibrary';

export function TemplatePreviewCard({ template, compact = false }: { template: PdfTemplate; compact?: boolean }) {
  if (template.kind === 'imported-pdf') {
    return (
      <div className="flex flex-col gap-2" data-testid="template-preview-card">
        <div className={compact ? 'flex h-32 items-center justify-center rounded-lg bg-muted/50' : 'flex h-48 items-center justify-center rounded-lg bg-muted/50'}>
          <div className="flex aspect-[1/1.414] h-[80%] items-center justify-center border border-border bg-white shadow-sm">
            <span className="text-xs font-medium text-black">PDF</span>
          </div>
        </div>
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="truncate text-sm font-medium">{template.name}</span>
          <span className="text-xs text-muted-foreground">{templateSummary(template)}</span>
        </div>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-2" data-testid="template-preview-card">
      <BlankPdfPagePreview settings={template.settings} compact={compact} />
      <div className="flex min-w-0 items-start justify-between gap-2">
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="truncate text-sm font-medium">{template.name}</span>
          <span className="text-xs text-muted-foreground">{templateSummary(template)}</span>
        </div>
        {template.settings.patternType !== 'blank' ? <Badge variant="secondary">{templateGridSummary(template)}</Badge> : null}
      </div>
    </div>
  );
}
