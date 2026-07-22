import {
  MENU_DROPDOWN,
  MENU_ITEM_DEFAULT,
  MENU_ITEM_DISABLED,
} from './shellSpacing';

export interface MenuItem {
  label: string;
  onSelect?: () => void;
  disabled?: boolean;
  testId?: string;
}

interface MenuDropdownProps {
  items: readonly MenuItem[];
}

export function MenuDropdown({ items }: MenuDropdownProps) {
  return (
    <div className={['absolute left-0 top-[calc(100%+4px)] z-50 min-w-[168px] rounded-[6px] border p-1', MENU_DROPDOWN].join(' ')}>
      {items.map((item) => (
        <button
          key={item.label}
          type="button"
          className={[
            'flex h-7 w-full items-center rounded-[4px] px-2 text-left text-[12px] transition',
            item.disabled ? MENU_ITEM_DISABLED : MENU_ITEM_DEFAULT,
          ].join(' ')}
          disabled={item.disabled}
          data-testid={item.testId}
          onClick={item.onSelect}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
