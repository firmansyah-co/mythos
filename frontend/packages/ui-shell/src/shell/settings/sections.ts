// The sections of the settings, in the order they are listed.
//
// A section names the least role that may open it; the controller enforces the same on its
// routes, so hiding a section is a courtesy, not the protection. A new kind of setting is a new
// entry here and a component.

import type { ComponentType } from 'react';
import { UserRound, Users } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import type { Role } from '../../api/client';
import { AccountSettings } from './AccountSettings';
import { UsersSettings } from './UsersSettings';

export interface SettingsSection {
  id: string;
  label: string;
  icon: LucideIcon;
  /** The least role that may open it; `null` for every signed-in account. */
  minRole: Role | null;
  component: ComponentType;
}

export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  { id: 'account', label: 'Account', icon: UserRound, minRole: null, component: AccountSettings },
  { id: 'users', label: 'Users', icon: Users, minRole: 'admin', component: UsersSettings },
];
