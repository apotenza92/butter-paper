import { useEffect, useMemo, useRef, useState } from 'react';
import { MenuDropdown, type MenuItem } from './MenuDropdown';
import { MenuTrigger } from './MenuTrigger';
import {
  MENU_BAR_HEIGHT,
  SHELL_BORDER_SUBTLE,
  SHELL_CONTROL_GAP,
  SHELL_ROW_INSET_X,
  SHELL_SURFACE_PANEL,
} from './shellSpacing';

type MenuKey = 'butter-paper' | 'file' | 'edit' | 'view' | 'document' | null;

interface AppMenuBarProps {
  canSave: boolean;
  onOpen: () => void;
  onOpenCanvas: () => void;
  onSave: () => void;
  onSaveAs: () => void;
  onSetPageScale: () => void;
}

export function AppMenuBar({ canSave, onOpen, onOpenCanvas, onSave, onSaveAs, onSetPageScale }: AppMenuBarProps) {
  const [openMenu, setOpenMenu] = useState<MenuKey>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!openMenu) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpenMenu(null);
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpenMenu(null);
      }
    };

    window.addEventListener('pointerdown', handlePointerDown);
    window.addEventListener('keydown', handleKeyDown);

    return () => {
      window.removeEventListener('pointerdown', handlePointerDown);
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [openMenu]);

  const placeholderItems = useMemo(() => {
    return [
      { label: 'Coming soon', disabled: true },
      { label: 'Available in a later pass', disabled: true },
    ];
  }, []);

  const fileItems = useMemo(() => {
    return [
      {
        label: 'Open...',
        onSelect: () => {
          setOpenMenu(null);
          onOpen();
        },
        testId: 'menu-file-open',
      },
      {
        label: 'Open Butter Canvas...',
        onSelect: () => {
          setOpenMenu(null);
          onOpenCanvas();
        },
        testId: 'menu-file-open-canvas',
      },
      {
        label: 'Save',
        disabled: !canSave,
        onSelect: canSave
          ? () => {
              setOpenMenu(null);
              onSave();
            }
          : undefined,
        testId: 'menu-file-save',
      },
      {
        label: 'Save As...',
        disabled: !canSave,
        onSelect: canSave
          ? () => {
              setOpenMenu(null);
              onSaveAs();
            }
          : undefined,
        testId: 'menu-file-save-as',
      },
    ];
  }, [canSave, onOpen, onOpenCanvas, onSave, onSaveAs]);

  const documentItems = useMemo(() => {
    return [
      {
        label: 'Set Page Scale...',
        disabled: !canSave,
        onSelect: canSave
          ? () => {
              setOpenMenu(null);
              onSetPageScale();
            }
          : undefined,
        testId: 'menu-document-set-page-scale',
      },
    ];
  }, [canSave, onSetPageScale]);

  const menus: Array<{ key: Exclude<MenuKey, null>; label: string; items: MenuItem[] }> = [
    { key: 'butter-paper', label: 'Butter Paper', items: placeholderItems },
    { key: 'file', label: 'File', items: fileItems },
    { key: 'edit', label: 'Edit', items: placeholderItems },
    { key: 'view', label: 'View', items: placeholderItems },
    { key: 'document', label: 'Document', items: documentItems },
  ];

  return (
    <div
      ref={rootRef}
      className={[
        'flex items-center border-b',
        MENU_BAR_HEIGHT,
        SHELL_ROW_INSET_X,
        SHELL_CONTROL_GAP,
        SHELL_SURFACE_PANEL,
        SHELL_BORDER_SUBTLE,
      ].join(' ')}
      data-testid="app-menu-bar"
    >
      {menus.map((menu) => (
        <div key={menu.key} className="relative">
          <MenuTrigger
            active={openMenu === menu.key}
            label={menu.label}
            onClick={() => setOpenMenu((current) => (current === menu.key ? null : menu.key))}
            testId={`menu-trigger-${menu.key}`}
          />
          {openMenu === menu.key ? <MenuDropdown items={menu.items} /> : null}
        </div>
      ))}
    </div>
  );
}
