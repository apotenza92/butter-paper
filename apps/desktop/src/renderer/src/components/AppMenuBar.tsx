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
  MenubarShortcut,
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
  accelerator?: string;
  onSelect?: () => void;
  disabled?: boolean;
  testId?: string;
}

interface AppMenuBarProps {
  canSave: boolean;
  productName: string;
  updateStatus: UpdateStatus | null;
  menuBarVisible: boolean;
  showMenuBarVisibilityOption: boolean;
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
  onSaveDocumentAsTemplate?: () => void;
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
export const APP_MENU_SHORTCUT_CLASS_NAME = 'ml-auto min-w-24 pl-8 text-right';
export const APP_MENU_KEYS: readonly MenuKey[] = ['butter-paper', 'file', 'edit', 'view'];

export function AppMenuBar({ canSave, productName, updateStatus, menuBarVisible, showMenuBarVisibilityOption, canUndo, canRedo, canCut, canCopy, canPaste, canSelectAll, onNewPdf, onOpen, onSave, onSaveAs, onSaveDocumentAsTemplate, onUndo, onRedo, onCut, onCopy, onPaste, onSelectAll, onMenuBarVisibilityChange, onReload, onForceReload, onToggleFullScreen, onSetAsDefaultPdfApp, onCheckForUpdates, onOpenReleasePage, onUpdateFrequencyChange, onQuit }: AppMenuBarProps) {
  const fileItems = useMemo<AppMenuItem[]>(() => {
    return [
      {
        label: APPLICATION_MENU_COMMANDS.newPdf.label,
        accelerator: APPLICATION_MENU_COMMANDS.newPdf.accelerator,
        onSelect: onNewPdf,
        testId: 'menu-file-new-pdf',
      },
      {
        label: APPLICATION_MENU_COMMANDS.openPdf.label,
        accelerator: APPLICATION_MENU_COMMANDS.openPdf.accelerator,
        onSelect: onOpen,
        testId: 'menu-file-open',
      },
      {
        label: APPLICATION_MENU_COMMANDS.save.label,
        accelerator: APPLICATION_MENU_COMMANDS.save.accelerator,
        disabled: !canSave,
        onSelect: canSave ? onSave : undefined,
        testId: 'menu-file-save',
      },
      {
        label: APPLICATION_MENU_COMMANDS.saveAs.label,
        accelerator: APPLICATION_MENU_COMMANDS.saveAs.accelerator,
        disabled: !canSave,
        onSelect: canSave ? onSaveAs : undefined,
        testId: 'menu-file-save-as',
      },
      {
        label: APPLICATION_MENU_COMMANDS.saveDocumentAsTemplate.label,
        disabled: !canSave,
        onSelect: canSave ? onSaveDocumentAsTemplate : undefined,
        testId: 'menu-file-save-as-template',
      },
    ];
  }, [canSave, onNewPdf, onOpen, onSave, onSaveAs, onSaveDocumentAsTemplate]);

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
                    <AppMenuShortcut />
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
                    <AppMenuShortcut />
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
                    <AppMenuShortcut />
                  </MenubarItem>
                  {updateStatus?.disabledReason ? (
                    <MenubarItem disabled>
                      {updateDisabledReasonLabel(updateStatus.disabledReason)}
                      <AppMenuShortcut />
                    </MenubarItem>
                  ) : null}
                </MenubarGroup>
                <MenubarSeparator />
                <MenubarGroup>
                  <MenubarItem data-testid="menu-quit" onClick={onQuit}>
                    <X aria-hidden="true" />
                    Quit {productName}
                    <AppMenuShortcut />
                  </MenubarItem>
                </MenubarGroup>
              </>
            ) : menu.key === 'edit' ? (
              <>
                <MenubarGroup>
                  <MenubarItem disabled={!canUndo} onClick={onUndo}>
                    {APPLICATION_MENU_COMMANDS.undo.label}
                    <AppMenuShortcut accelerator={APPLICATION_MENU_COMMANDS.undo.accelerator} />
                  </MenubarItem>
                  <MenubarItem disabled={!canRedo} onClick={onRedo}>
                    {APPLICATION_MENU_COMMANDS.redo.label}
                    <AppMenuShortcut accelerator={APPLICATION_MENU_COMMANDS.redo.accelerator} />
                  </MenubarItem>
                </MenubarGroup>
                <MenubarSeparator />
                <MenubarGroup>
                  <MenubarItem disabled={!canCut} onClick={onCut}>
                    {APPLICATION_MENU_COMMANDS.cut.label}
                    <AppMenuShortcut accelerator={APPLICATION_MENU_COMMANDS.cut.accelerator} />
                  </MenubarItem>
                  <MenubarItem disabled={!canCopy} onClick={onCopy}>
                    {APPLICATION_MENU_COMMANDS.copy.label}
                    <AppMenuShortcut accelerator={APPLICATION_MENU_COMMANDS.copy.accelerator} />
                  </MenubarItem>
                  <MenubarItem disabled={!canPaste} onClick={onPaste}>
                    {APPLICATION_MENU_COMMANDS.paste.label}
                    <AppMenuShortcut accelerator={APPLICATION_MENU_COMMANDS.paste.accelerator} />
                  </MenubarItem>
                  <MenubarItem disabled={!canSelectAll} onClick={onSelectAll}>
                    {APPLICATION_MENU_COMMANDS.selectAll.label}
                    <AppMenuShortcut accelerator={APPLICATION_MENU_COMMANDS.selectAll.accelerator} />
                  </MenubarItem>
                </MenubarGroup>
              </>
            ) : menu.key === 'view' ? (
              <>
                {showMenuBarVisibilityOption ? (
                  <>
                    <MenubarGroup>
                      <MenubarCheckboxItem
                        checked={menuBarVisible}
                        onCheckedChange={(checked) => onMenuBarVisibilityChange(checked === true)}
                      >
                        {APPLICATION_MENU_BAR_VISIBILITY_LABEL}
                        <AppMenuShortcut />
                      </MenubarCheckboxItem>
                    </MenubarGroup>
                    <MenubarSeparator />
                  </>
                ) : null}
                <MenubarGroup>
                  <MenubarItem onClick={onReload}>
                    Reload
                    <AppMenuShortcut />
                  </MenubarItem>
                  <MenubarItem onClick={onForceReload}>
                    Force Reload
                    <AppMenuShortcut />
                  </MenubarItem>
                  <MenubarItem onClick={onToggleFullScreen}>
                    Toggle Full Screen
                    <AppMenuShortcut />
                  </MenubarItem>
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
                    <AppMenuShortcut accelerator={item.accelerator} />
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

interface AppMenuShortcutProps {
  accelerator?: string;
}

function AppMenuShortcut({ accelerator }: AppMenuShortcutProps) {
  return (
    <MenubarShortcut aria-hidden="true" className={APP_MENU_SHORTCUT_CLASS_NAME}>
      {accelerator ? formatMenuAccelerator(accelerator) : null}
    </MenubarShortcut>
  );
}

export function formatMenuAccelerator(accelerator: string, applePlatform = isApplePlatform()): string {
  const keys = accelerator.split('+');
  if (!applePlatform) {
    return keys.map((key) => key === 'CommandOrControl' ? 'Ctrl' : key).join('+');
  }
  const symbols: Record<string, string> = {
    Alt: '⌥',
    Command: '⌘',
    CommandOrControl: '⌘',
    Control: '⌃',
    Shift: '⇧',
  };
  const modifierOrder = ['Control', 'Alt', 'Shift', 'Command', 'CommandOrControl'];
  const modifiers = keys
    .filter((key) => modifierOrder.includes(key))
    .sort((left, right) => modifierOrder.indexOf(left) - modifierOrder.indexOf(right));
  const primaryKeys = keys.filter((key) => !modifierOrder.includes(key));
  return [...modifiers, ...primaryKeys]
    .map((key) => symbols[key] ?? key)
    .join('');
}

function isApplePlatform(): boolean {
  return typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
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
