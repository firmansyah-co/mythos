import { useEffect, useRef, useState } from 'react';
import { Check } from 'lucide-react';

export interface MenuItem {
  label: string;
  run: () => void;
  disabled?: boolean;
  /** For toggles: whether the option is currently on. */
  checked?: boolean;
}

export interface Menu {
  label: string;
  items: MenuItem[];
}

/** The menu bar. Every entry does something: nothing is listed that is not implemented. */
export function MenuBar({ menus }: { menus: Menu[] }) {
  const [open, setOpen] = useState<number | null>(null);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open === null) return;
    const close = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(null);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(null);
    };
    document.addEventListener('mousedown', close);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', close);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div className="menubar" role="menubar" ref={root}>
      {menus.map((menu, index) => (
        <div key={menu.label} className="menubar__menu">
          <button
            type="button"
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={open === index}
            className={`menubar__title${open === index ? ' is-open' : ''}`}
            onClick={() => setOpen(open === index ? null : index)}
            onMouseEnter={() => open !== null && setOpen(index)}
          >
            {menu.label}
          </button>
          {open === index && (
            <div className="menubar__list" role="menu" aria-label={menu.label}>
              {menu.items.map((item) => (
                <button
                  key={item.label}
                  type="button"
                  role={item.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
                  aria-checked={item.checked}
                  disabled={item.disabled}
                  className="menubar__item"
                  onClick={() => {
                    setOpen(null);
                    item.run();
                  }}
                >
                  <span className="menubar__check">{item.checked ? <Check size={13} /> : null}</span>
                  {item.label}
                </button>
              ))}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
