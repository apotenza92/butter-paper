import { useMemo } from 'react';
import {
  Menubar,
  MenubarContent,
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
import {
  MENU_BAR_HEIGHT,
  MENU_TRIGGER_HEIGHT,
  SHELL_BORDER_SUBTLE,
  SHELL_CONTROL_GAP,
  SHELL_ROW_INSET_X,
  SHELL_SURFACE_PANEL,
} from './shellSpacing';

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
  onOpen: () => void;
  onOpenCanvas: () => void;
  onSave: () => void;
  onSaveAs: () => void;
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

export function AppMenuBar({ canSave, productName, updateStatus, onOpen, onOpenCanvas, onSave, onSaveAs, onCheckForUpdates, onOpenReleasePage, onUpdateFrequencyChange }: AppMenuBarProps) {
  const fileItems = useMemo<AppMenuItem[]>(() => {
    return [
      {
        label: 'Open...',
        onSelect: onOpen,
        testId: 'menu-file-open',
      },
      {
        label: 'Open Butter Canvas...',
        onSelect: onOpenCanvas,
        testId: 'menu-file-open-canvas',
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
  }, [canSave, onOpen, onOpenCanvas, onSave, onSaveAs]);

  const menus: Array<{ key: MenuKey; label: string; items: AppMenuItem[] }> = [
    { key: 'butter-paper', label: productName, items: [] },
    { key: 'file', label: 'File', items: fileItems },
  ];

  return (
    <Menubar
      className={[
        'w-full justify-start rounded-none border-0 border-b py-0',
        MENU_BAR_HEIGHT,
        SHELL_ROW_INSET_X,
        SHELL_CONTROL_GAP,
        SHELL_SURFACE_PANEL,
        SHELL_BORDER_SUBTLE,
      ].join(' ')}
      data-testid="app-menu-bar"
    >
      {menus.map((menu) => (
        <MenubarMenu key={menu.key}>
          <MenubarTrigger
            className={['px-2 py-0 text-[12px]', MENU_TRIGGER_HEIGHT].join(' ')}
            data-testid={`menu-trigger-${menu.key}`}
          >
            {menu.label}
          </MenubarTrigger>
          <MenubarContent className="min-w-[168px]">
            {menu.key === 'butter-paper' ? (
              <>
                <MenubarItem
                  className="h-7 py-0 text-[12px]"
                  data-testid="menu-check-for-updates"
                  disabled={!updateStatus?.enabled || updateStatus.phase === 'checking' || updateStatus.phase === 'downloading'}
                  onClick={onCheckForUpdates}
                >
                  {updateStatus?.phase === 'checking' ? 'Checking for Updates...' : 'Check for Updates...'}
                </MenubarItem>
                <MenubarSub>
                  <MenubarSubTrigger
                    className="h-7 py-0 text-[12px]"
                    data-testid="menu-update-frequency"
                    disabled={!updateStatus}
                  >
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
                          className="h-7 py-0 text-[12px]"
                          data-testid={`menu-update-frequency-${frequency.value}`}
                          value={frequency.value}
                        >
                          {frequency.label}
                        </MenubarRadioItem>
                      ))}
                    </MenubarRadioGroup>
                  </MenubarSubContent>
                </MenubarSub>
                <MenubarItem
                  className="h-7 py-0 text-[12px]"
                  data-testid="menu-open-release-page"
                  onClick={onOpenReleasePage}
                >
                  View Releases...
                </MenubarItem>
                <MenubarSeparator />
                <MenubarItem className="h-7 py-0 text-[12px]" disabled>
                  {updateStatus
                    ? `Version ${updateStatus.currentVersion}${updateStatus.channel === 'beta' ? ' Beta' : ''}`
                    : 'Loading update settings...'}
                </MenubarItem>
                {updateStatus?.disabledReason ? (
                  <MenubarItem className="h-7 py-0 text-[12px]" disabled>
                    {updateDisabledReasonLabel(updateStatus.disabledReason)}
                  </MenubarItem>
                ) : null}
              </>
            ) : menu.items.map((item) => (
              <MenubarItem
                key={item.label}
                className="h-7 py-0 text-[12px]"
                data-testid={item.testId}
                disabled={item.disabled}
                onClick={item.onSelect}
              >
                {item.label}
              </MenubarItem>
            ))}
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
