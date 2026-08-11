import { Separator } from '@/components/ui/separator';

interface WindowTitleBarProps {
  title: string;
}

export function formatWindowTitle(applicationTitle: string, documentName?: string, otherTabCount = 0): string {
  if (!documentName) return applicationTitle;
  const tabCountLabel = otherTabCount > 0 ? ` (+${otherTabCount})` : '';
  return `${documentName}${tabCountLabel} — ${applicationTitle}`;
}

export function WindowTitleBar({ title }: WindowTitleBarProps) {
  return (
    <header className="bp-window-titlebar relative" data-testid="window-title-bar">
      <span className="max-w-full truncate px-2 text-xs font-medium text-muted-foreground">
        {title}
      </span>
      <Separator className="absolute inset-x-0 bottom-0" data-testid="window-title-bar-separator" />
    </header>
  );
}
