import { readFileSync } from 'node:fs';

describe('repository hygiene release guardrails', () => {
  it('mechanically rejects the obsolete package workflow and retired credential names', () => {
    const hygiene = readFileSync('scripts/check-repository-hygiene.mjs', 'utf8');
    expect(hygiene).toContain("'.github/workflows/packages.yml'");
    for (const retired of [
      'MACOS_CSC_LINK',
      'CSC_LINK',
      'APPLE_ID',
      'APPLE_APP_SPECIFIC_PASSWORD',
      'APPLE_ID_PASSWORD',
    ]) {
      expect(hygiene).toContain(retired);
    }
  });

  it('checks maintained package and workflow command paths for dangling references', () => {
    const hygiene = readFileSync('scripts/check-repository-hygiene.mjs', 'utf8');
    expect(hygiene).toContain('references missing maintained path');
    expect(hygiene).toContain("filePath.startsWith('.github/workflows/')");
    expect(hygiene).toContain("filePath === 'apps/desktop/package.json'");
    expect(hygiene).toContain('third-party action is not pinned to a full commit SHA');
    expect(hygiene).toContain('/@[a-f0-9]{40}$/');
  });

  it('pins the desktop renderer to the stock Base UI Nova preset and Geist', () => {
    const components = JSON.parse(readFileSync('apps/desktop/components.json', 'utf8'));
    const desktopPackage = JSON.parse(readFileSync('apps/desktop/package.json', 'utf8'));
    const styles = readFileSync('apps/desktop/src/renderer/src/styles.css', 'utf8');
    const audit = readFileSync('scripts/audit-shadcn-registry.mjs', 'utf8');
    const repositoryInstructions = readFileSync('AGENTS.md', 'utf8');

    expect(components.style).toBe('base-nova');
    expect(components.iconLibrary).toBe('lucide');
    expect(desktopPackage.dependencies['@base-ui/react']).toBe('1.6.0');
    expect(desktopPackage.dependencies['@fontsource-variable/geist']).toBe('5.3.0');
    expect(desktopPackage.devDependencies.shadcn).toBe('4.16.1');
    expect(desktopPackage.dependencies['@fontsource/dm-sans']).toBeUndefined();
    expect(desktopPackage.devDependencies['@fontsource/dm-sans']).toBeUndefined();
    expect(styles).toContain('@import "@fontsource-variable/geist";');
    expect(styles).toContain("--font-sans: 'Geist Variable', sans-serif;");
    expect(styles).not.toMatch(/DM Sans|--bp-(?:surface|border-subtle|selected-neutral|text-(?:primary|secondary|muted))/);
    expect(audit).toContain("style: 'base-nova'");
    expect(audit).toContain("preset: 'b2fA'");
    expect(repositoryInstructions).toContain('Base UI primitives and the Nova style');
    expect(repositoryInstructions).not.toContain('style: "base-rhea"');
  });

  it('keeps the closable document tab as an explicit domain exception', () => {
    const hygiene = readFileSync('scripts/check-repository-hygiene.mjs', 'utf8');
    const documentTabBar = readFileSync('apps/desktop/src/renderer/src/components/DocumentTabBar.tsx', 'utf8');
    const blankPdfSettings = readFileSync('apps/desktop/src/renderer/src/components/BlankPdfSettingsPopover.tsx', 'utf8');
    const blankPdfSettingsFields = readFileSync('apps/desktop/src/renderer/src/components/BlankPdfSettingsFields.tsx', 'utf8');
    const newBlankPdfDialog = readFileSync('apps/desktop/src/renderer/src/components/NewBlankPdfDialog.tsx', 'utf8');
    const appRenderer = readFileSync('apps/desktop/src/renderer/src/app.tsx', 'utf8');
    const closableTab = readFileSync('apps/desktop/src/renderer/src/components/domain-ui/ClosableDocumentTab.tsx', 'utf8');
    const splitButtonSegment = readFileSync('apps/desktop/src/renderer/src/components/domain-ui/SplitButtonSegment.tsx', 'utf8');
    const styles = readFileSync('apps/desktop/src/renderer/src/styles.css', 'utf8');

    expect(hygiene).toContain('ClosableDocumentTab.tsx');
    expect(hygiene).toContain('shadcn Tabs has no closable document-tab pattern');
    expect(closableTab).toContain('data-domain-ui-exception="closable-document-tab"');
    expect(closableTab).toContain('<TabsTrigger');
    expect(documentTabBar).not.toContain('variant="line"');
    expect(documentTabBar).toContain('className="shrink-0 justify-start gap-2 rounded-none bg-background! p-0! group-data-horizontal/tabs:h-8!"');
    expect(documentTabBar).toContain('data-testid="document-tab-surface"');
    expect(documentTabBar).toContain('data-testid="document-tab-actions"');
    expect(documentTabBar).toContain('className="flex items-center border-b border-border bg-background p-2"');
    expect(documentTabBar).toContain('className="bp-native-scroll-hidden flex min-w-0 items-center gap-2 overflow-x-auto"');
    expect(documentTabBar).toContain('className="flex h-8 shrink-0 items-center gap-2 bg-background"');
    expect(documentTabBar).not.toContain('data-tab-line');
    expect(documentTabBar).toContain('<Separator');
    expect(documentTabBar).toContain('data-testid="document-tab-actions-separator"');
    expect(documentTabBar).not.toContain('variant="ghost"');
    expect(documentTabBar.match(/<SplitButtonSegment/g)).toHaveLength(2);
    expect(documentTabBar).toContain('<Plus data-icon="inline-start" aria-hidden="true" />');
    expect(documentTabBar).toContain('aria-label="Open PDF"');
    expect(documentTabBar.match(/size="icon"/g)).toHaveLength(2);
    expect(documentTabBar).toContain('<ButtonGroup aria-label="New blank PDF controls">');
    expect(documentTabBar).not.toContain('<ButtonGroupSeparator />');
    expect(blankPdfSettings).toContain('<Popover open={open} onOpenChange={handleOpenChange}>');
    expect(blankPdfSettings).toContain('<SplitButtonSegment');
    expect(blankPdfSettings).toContain('data-testid="document-tab-new-pdf-settings"');
    expect(blankPdfSettings).toContain('data-testid="new-blank-pdf-settings"');
    expect(blankPdfSettings).toContain('<BlankPdfSettingsFields');
    expect(blankPdfSettings).not.toContain('Change default');
    expect(blankPdfSettingsFields).toContain('formatBlankPdfPaperPresetOption(value, settings.orientation)');
    expect(newBlankPdfDialog).toContain('<DialogTitle>New Blank PDF</DialogTitle>');
    expect(newBlankPdfDialog).toContain('testIdPrefix="new-blank-pdf-dialog"');
    expect(newBlankPdfDialog).toContain('data-testid="new-blank-pdf-dialog-create"');
    expect(appRenderer).toContain('onNewPdf={() => setNewBlankPdfDialogOpen(true)}');
    expect(appRenderer).toContain('onNewPdf={() => void handleCreateDefaultBlankPdf()}');
    expect(documentTabBar).not.toContain('<div className="min-w-0 flex-1" aria-hidden="true"');
    expect(closableTab).not.toMatch(/\bdata-active=|\bjustify-start\b|\bmin-w-24\b|\bmax-w-\[|\bflex-none\b|\btouch-none\b|\bcursor-default\b/);
    expect(closableTab).toContain('className="h-8! bg-background! data-active:bg-muted! group-data-[dragging]/document-tab:after:opacity-0!"');
    expect(closableTab).toContain('className="inline-flex h-full shrink-0 items-center text-muted-foreground leading-none"');
    expect(splitButtonSegment).toContain('data-domain-ui-exception="split-button-segment"');
    expect(splitButtonSegment).toContain('dark:aria-expanded:bg-transparent!');
    expect(splitButtonSegment).toContain('dark:aria-expanded:bg-muted!');
    expect(styles).not.toMatch(/\[data-domain-ui-exception="closable-document-tab"\][^{]*\[data-slot="tabs-trigger"\]/);
  });

  it('keeps shell controls and top rail actions on stock neutral Nova treatments', () => {
    const leftRail = readFileSync('apps/desktop/src/renderer/src/components/LeftRail.tsx', 'utf8');
    const rightRail = readFileSync('apps/desktop/src/renderer/src/components/RightRail.tsx', 'utf8');
    const snapSettings = readFileSync('apps/desktop/src/renderer/src/components/SnapSettingsMenu.tsx', 'utf8');
    const menuBar = readFileSync('apps/desktop/src/renderer/src/components/AppMenuBar.tsx', 'utf8');
    const viewerToolbar = readFileSync('apps/desktop/src/renderer/src/components/ViewerToolbar.tsx', 'utf8');

    expect(leftRail).not.toContain('variant="outline"');
    expect(rightRail).not.toContain('variant="outline"');
    expect(snapSettings.match(/variant="outline"/g)).toHaveLength(3);
    expect(leftRail).not.toContain('RailSettingsPopover');
    expect(rightRail).not.toContain('RailSettingsPopover');
    expect(leftRail).not.toContain('data-expanded');
    expect(rightRail).not.toContain('data-expanded');
    expect(rightRail).toContain("const TOP_RAIL_TOOL_IDS = ['select', 'pan'] as const;");
    expect(rightRail).not.toContain("{ group: 'general', heading: 'General' }");
    expect(rightRail).toContain('data-testid="right-rail-general-heading"');
    expect(rightRail).toContain("{ group: 'markup', heading: 'Markup' }");
    expect(rightRail).toContain("{ group: 'draw', heading: 'Draw' }");
    expect(rightRail).toContain("{ group: 'measure', heading: 'Measure' }");
    expect(rightRail).toContain('{shouldShowRightRailHeadings(columnCount) ? (');
    expect(rightRail).toContain('data-testid={`right-rail-group-divider-${group}`}');
    expect(menuBar).toContain('data-testid="menu-quit"');
    expect(menuBar).toContain('<X aria-hidden="true" />');
    expect(menuBar).toContain('Quit {productName}');
    expect(menuBar).not.toContain('updateStatus.currentVersion');
    expect(menuBar).toContain('className="w-full justify-start rounded-none border-x-0 border-t-0"');
    expect(viewerToolbar).toContain('onDoubleClick');
    expect(viewerToolbar).toContain('Double click to view Continuous');
    expect(viewerToolbar).toContain('Double click to view Single Page');
    expect(viewerToolbar).toContain('Double click to Fit Width');
    expect(viewerToolbar).toContain('Double click to Fit Page');
    expect(viewerToolbar).toContain('<SplitButtonSegment');
    expect(viewerToolbar).toContain('<DropdownMenuGroup>\n            <DropdownMenuLabel>Mousewheel Behaviour</DropdownMenuLabel>');
  });

  it('routes the Butter Paper quit menu through the protected application close flow', () => {
    const menuBar = readFileSync('apps/desktop/src/renderer/src/components/AppMenuBar.tsx', 'utf8');
    const appRenderer = readFileSync('apps/desktop/src/renderer/src/app.tsx', 'utf8');
    const protocol = readFileSync('apps/desktop/src/shared/protocol.ts', 'utf8');
    const channels = readFileSync('apps/desktop/src/shared/ipc.ts', 'utf8');
    const preload = readFileSync('apps/desktop/src/preload/index.ts', 'utf8');
    const mainWindow = readFileSync('apps/desktop/src/main/window.ts', 'utf8');

    expect(menuBar).toContain('data-testid="menu-quit"');
    expect(menuBar).toContain('onClick={onQuit}');
    expect(menuBar).not.toContain('updateStatus.currentVersion');
    expect(appRenderer).toContain('window.butterPaper.application.requestQuit()');
    expect(protocol).toContain('requestQuit(): Promise<void>');
    expect(channels).toContain("applicationRequestQuit: 'application:request-quit'");
    expect(preload).toContain('ipcRenderer.invoke(ipcChannels.applicationRequestQuit)');
    expect(mainWindow).toContain('ipcMain.handle(ipcChannels.applicationRequestQuit');
    expect(mainWindow).toContain('app.quit();');
  });
});
