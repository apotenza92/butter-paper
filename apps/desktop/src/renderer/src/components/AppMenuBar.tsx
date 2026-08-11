import { useMemo } from 'react';
import { X } from 'lucide-react';
import {
  Menubar,
  MenubarContent,
  MenubarCheckboxItem,
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
import {
  APPLICATION_MENU_BAR_VISIBILITY_LABEL,
  APPLICATION_MENU_COMMANDS,
  APPLICATION_MENU_UPDATE_FREQUENCIES,
  updateCheckMenuLabel,
} from '../../../shared/applicationMenu';

type MenuKey = 'butter-paper' | 'file' | 'edit' | 'view';

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
  menuBarVisible: boolean;
  canUndo: boolean;
  canRedo: boolean;
  canCut: boolean;
  canCopy: boolean;
  canPaste: boolean;
  canSelectAll: boolean;
  onNewPdf: () => void;
  onOpen: () => void;
  onSave: () => void;
  onSaveAs: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onCut: () => void;
  onCopy: () => void;
  onPaste: () => void;
  onSelectAll: () => void;
  onMenuBarVisibilityChange: (visible: boolean) => void;
  onReload: () => void;
  onForceReload: () => void;
  onToggleFullScreen: () => void;
  onSetAsDefaultPdfApp: () => void;
  onCheckForUpdates: () => void;
  onOpenReleasePage: () => void;
  onUpdateFrequencyChange: (frequency: UpdateFrequency) => void;
  onQuit: () => void;
}

export const APP_MENU_CONTENT_CLASS_NAME = 'w-max whitespace-nowrap';
export const APP_MENU_KEYS: readonly MenuKey[] = ['butter-paper', 'file', 'edit', 'view'];

export function AppMenuBar({ canSave, productName, updateStatus, menuBarVisible, canUndo, canRedo, canCut, canCopy, canPaste, canSelectAll, onNewPdf, onOpen, onSave, onSaveAs, onUndo, onRedo, onCut, onCopy, onPaste, onSelectAll, onMenuBarVisibilityChange, onReload, onForceReload, onToggleFullScreen, onSetAsDefaultPdfApp, onCheckForUpdates, onOpenReleasePage, onUpdateFrequencyChange, onQuit }: AppMenuBarProps) {
  const fileItems = useMemo<AppMenuItem[]>(() => {
    return [
      {
        label: APPLICATION_MENU_COMMANDS.newPdf.label,
        onSelect: onNewPdf,
        testId: 'menu-file-new-pdf',
      },
      {
        label: APPLICATION_MENU_COMMANDS.openPdf.label,
        onSelect: onOpen,
        testId: 'menu-file-open',
      },
      {
        label: APPLICATION_MENU_COMMANDS.save.label,
        disabled: !canSave,
        onSelect: canSave ? onSave : undefined,
        testId: 'menu-file-save',
      },
      {
        label: APPLICATION_MENU_COMMANDS.saveAs.label,
        disabled: !canSave,
        onSelect: canSave ? onSaveAs : undefined,
        testId: 'menu-file-save-as',
      },
    ];
  }, [canSave, onNewPdf, onOpen, onSave, onSaveAs]);

  const menus: Array<{ key: MenuKey; label: string; items: AppMenuItem[] }> = [
    { key: 'butter-paper', label: productName, items: [] },
    { key: 'file', label: 'File', items: fileItems },
    { key: 'edit', label: 'Edit', items: [] },
    { key: 'view', label: 'View', items: [] },
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
          <MenubarContent className={APP_MENU_CONTENT_CLASS_NAME}>
            {menu.key === 'butter-paper' ? (
              <>
                <MenubarGroup>
                  <MenubarItem data-testid="menu-set-default-pdf-app" onClick={onSetAsDefaultPdfApp}>
                    {APPLICATION_MENU_COMMANDS.setDefaultPdfApp.label}
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
                    {updateCheckMenuLabel(updateStatus?.phase ?? 'idle', updateStatus?.downloadPercent ?? null)}
                  </MenubarItem>
                  <MenubarSub>
                    <MenubarSubTrigger data-testid="menu-update-frequency" disabled={!updateStatus}>
                      Check Automatically
                    </MenubarSubTrigger>
                    <MenubarSubContent className={APP_MENU_CONTENT_CLASS_NAME}>
                      <MenubarRadioGroup
                        value={updateStatus?.frequency ?? 'daily'}
                        onValueChange={(value) => onUpdateFrequencyChange(value as UpdateFrequency)}
                      >
                        {APPLICATION_MENU_UPDATE_FREQUENCIES.map((frequency) => (
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
                    {APPLICATION_MENU_COMMANDS.openReleasePage.label}
                  </MenubarItem>
                  {updateStatus?.disabledReason ? (
                    <MenubarItem disabled>
                      {updateDisabledReasonLabel(updateStatus.disabledReason)}
                    </MenubarItem>
                  ) : null}
                </MenubarGroup>
                <MenubarSeparator />
                <MenubarGroup>
                  <MenubarItem data-testid="menu-quit" onClick={onQuit}>
                    <X aria-hidden="true" />
                    Quit {productName}
                  </MenubarItem>
                </MenubarGroup>
              </>
            ) : menu.key === 'edit' ? (
              <>
                <MenubarGroup>
                  <MenubarItem disabled={!canUndo} onClick={onUndo}>{APPLICATION_MENU_COMMANDS.undo.label}</MenubarItem>
                  <MenubarItem disabled={!canRedo} onClick={onRedo}>{APPLICATION_MENU_COMMANDS.redo.label}</MenubarItem>
                </MenubarGroup>
                <MenubarSeparator />
                <MenubarGroup>
                  <MenubarItem disabled={!canCut} onClick={onCut}>{APPLICATION_MENU_COMMANDS.cut.label}</MenubarItem>
                  <MenubarItem disabled={!canCopy} onClick={onCopy}>{APPLICATION_MENU_COMMANDS.copy.label}</MenubarItem>
                  <MenubarItem disabled={!canPaste} onClick={onPaste}>{APPLICATION_MENU_COMMANDS.paste.label}</MenubarItem>
                  <MenubarItem disabled={!canSelectAll} onClick={onSelectAll}>{APPLICATION_MENU_COMMANDS.selectAll.label}</MenubarItem>
                </MenubarGroup>
              </>
            ) : menu.key === 'view' ? (
              <>
                <MenubarGroup>
                  <MenubarCheckboxItem
                    checked={menuBarVisible}
                    onCheckedChange={(checked) => onMenuBarVisibilityChange(checked === true)}
                  >
                    {APPLICATION_MENU_BAR_VISIBILITY_LABEL}
                  </MenubarCheckboxItem>
                </MenubarGroup>
                <MenubarSeparator />
                <MenubarGroup>
                  <MenubarItem onClick={onReload}>Reload</MenubarItem>
                  <MenubarItem onClick={onForceReload}>Force Reload</MenubarItem>
                  <MenubarItem onClick={onToggleFullScreen}>Toggle Full Screen</MenubarItem>
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
