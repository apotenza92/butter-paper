import { useMemo } from 'react';
import {
  Menubar,
  MenubarContent,
  MenubarGroup,
  MenubarItem,
  MenubarMenu,
  MenubarRadioGroup,
  MenubarRadioItem,
  MenubarSeparator,
  MenubarSub,
  MenubarSubContent,
  MenubarSubTrigger,
  MenubarTrigger,
} from './ui/menubar';
import type { UpdateFrequency, UpdateStatus } from '../../../shared/protocol';

type MenuKey = 'butter-paper' | 'file';

interface AppMenuItem {
  label: string;
  onSelect?: () => void;
  disabled?: boolean;
  testId?: string;
}

interface AppMenuBarProps {
  canSave: boolean;
  productName: string;
  updateStatus: UpdateStatus | null;
  onNewPdf: () => void;
  onOpen: () => void;
  onSave: () => void;
  onSaveAs: () => void;
  onSetAsDefaultPdfApp: () => void;
  onCheckForUpdates: () => void;
  onOpenReleasePage: () => void;
  onUpdateFrequencyChange: (frequency: UpdateFrequency) => void;
}

const UPDATE_FREQUENCIES: Array<{ value: UpdateFrequency; label: string }> = [
  { value: 'never', label: 'Never' },
  { value: 'startup', label: 'At startup' },
  { value: 'hourly', label: 'Hourly' },
  { value: 'sixHours', label: 'Every 6 hours' },
  { value: 'twelveHours', label: 'Every 12 hours' },
  { value: 'daily', label: 'Daily' },
  { value: 'weekly', label: 'Weekly' },
  { value: 'monthly', label: 'Monthly' },
];

export function AppMenuBar({ canSave, productName, updateStatus, onNewPdf, onOpen, onSave, onSaveAs, onSetAsDefaultPdfApp, onCheckForUpdates, onOpenReleasePage, onUpdateFrequencyChange }: AppMenuBarProps) {
  const fileItems = useMemo<AppMenuItem[]>(() => {
    return [
      {
        label: 'New Blank PDF',
        onSelect: onNewPdf,
        testId: 'menu-file-new-pdf',
      },
      {
        label: 'Open...',
        onSelect: onOpen,
        testId: 'menu-file-open',
      },
      {
        label: 'Save',
        disabled: !canSave,
        onSelect: canSave ? onSave : undefined,
        testId: 'menu-file-save',
      },
      {
        label: 'Save As...',
        disabled: !canSave,
        onSelect: canSave ? onSaveAs : undefined,
        testId: 'menu-file-save-as',
      },
    ];
  }, [canSave, onNewPdf, onOpen, onSave, onSaveAs]);

  const menus: Array<{ key: MenuKey; label: string; items: AppMenuItem[] }> = [
    { key: 'butter-paper', label: productName, items: [] },
    { key: 'file', label: 'File', items: fileItems },
  ];

  return (
    <Menubar
      className="w-full justify-start rounded-none border-x-0 border-t-0"
      data-testid="app-menu-bar"
    >
      {menus.map((menu) => (
        <MenubarMenu key={menu.key}>
          <MenubarTrigger data-testid={`menu-trigger-${menu.key}`}>
            {menu.label}
          </MenubarTrigger>
          <MenubarContent className="min-w-[168px]">
            {menu.key === 'butter-paper' ? (
              <>
                <MenubarGroup>
                  <MenubarItem data-testid="menu-set-default-pdf-app" onClick={onSetAsDefaultPdfApp}>
                    Set as Default PDF App...
                  </MenubarItem>
                </MenubarGroup>
                <MenubarSeparator />
                <MenubarGroup>
                  <MenubarItem
                    data-testid="menu-check-for-updates"
                    disabled={!updateStatus?.enabled
                      || updateStatus.phase === 'checking'
                      || updateStatus.phase === 'available'
                      || updateStatus.phase === 'downloading'}
                    onClick={onCheckForUpdates}
                  >
                    {updateStatus?.phase === 'checking'
                      ? 'Checking for Updates...'
                      : updateStatus?.phase === 'available' || updateStatus?.phase === 'downloading'
                        ? `Downloading Update${updateStatus.downloadPercent == null
                          ? '...'
                          : ` (${Math.round(updateStatus.downloadPercent)}%)`}`
                        : updateStatus?.phase === 'downloaded'
                          ? 'Update Ready...'
                          : 'Check for Updates...'}
                  </MenubarItem>
                  <MenubarSub>
                    <MenubarSubTrigger data-testid="menu-update-frequency" disabled={!updateStatus}>
                      Check Automatically
                    </MenubarSubTrigger>
                    <MenubarSubContent>
                      <MenubarRadioGroup
                        value={updateStatus?.frequency ?? 'daily'}
                        onValueChange={(value) => onUpdateFrequencyChange(value as UpdateFrequency)}
                      >
                        {UPDATE_FREQUENCIES.map((frequency) => (
                          <MenubarRadioItem
                            key={frequency.value}
                            data-testid={`menu-update-frequency-${frequency.value}`}
                            value={frequency.value}
                          >
                            {frequency.label}
                          </MenubarRadioItem>
                        ))}
                      </MenubarRadioGroup>
                    </MenubarSubContent>
                  </MenubarSub>
                  <MenubarItem data-testid="menu-open-release-page" onClick={onOpenReleasePage}>
                    View Releases...
                  </MenubarItem>
                </MenubarGroup>
                <MenubarSeparator />
                <MenubarGroup>
                  <MenubarItem disabled>
                    {updateStatus
                      ? `Version ${updateStatus.currentVersion}${updateStatus.channel === 'beta' ? ' Beta' : ''}`
                      : 'Loading update settings...'}
                  </MenubarItem>
                  {updateStatus?.disabledReason ? (
                    <MenubarItem disabled>
                      {updateDisabledReasonLabel(updateStatus.disabledReason)}
                    </MenubarItem>
                  ) : null}
                </MenubarGroup>
              </>
            ) : (
              <MenubarGroup>
                {menu.items.map((item) => (
                  <MenubarItem
                    key={item.label}
                    data-testid={item.testId}
                    disabled={item.disabled}
                    onClick={item.onSelect}
                  >
                    {item.label}
                  </MenubarItem>
                ))}
              </MenubarGroup>
            )}
          </MenubarContent>
        </MenubarMenu>
      ))}
    </Menubar>
  );
}

function updateDisabledReasonLabel(reason: NonNullable<UpdateStatus['disabledReason']>): string {
  switch (reason) {
    case 'development':
      return 'Updates are available in packaged builds';
    case 'test-mode':
      return 'Updates are disabled during tests';
    case 'configuration':
      return 'Update feed is not configured';
    case 'platform-policy':
      return 'Use the release page to update on this platform';
  }
}
