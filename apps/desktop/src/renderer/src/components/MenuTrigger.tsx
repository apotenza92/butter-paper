import {
  MENU_TRIGGER_ACTIVE,
  MENU_TRIGGER_DEFAULT,
  MENU_TRIGGER_HEIGHT,
} from './shellSpacing';

interface MenuTriggerProps {
  active: boolean;
  label: string;
  onClick: () => void;
  testId?: string;
}

export function MenuTrigger({ active, label, onClick, testId }: MenuTriggerProps) {
  return (
    <button
      type="button"
      className={[
        'inline-flex items-center rounded-[6px] px-2 text-[12px] font-medium transition',
        MENU_TRIGGER_HEIGHT,
        active ? MENU_TRIGGER_ACTIVE : MENU_TRIGGER_DEFAULT,
      ].join(' ')}
      aria-expanded={active}
      data-testid={testId}
      onClick={onClick}
    >
      {label}
    </button>
  );
}
