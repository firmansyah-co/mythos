// The sections of the settings, in the order they are listed.
//
// A section names the least role that may open it; the controller enforces the same on its
// routes, so hiding a section is a courtesy, not the protection. A new kind of setting is a new
// entry here and a component.

import type { ComponentType } from 'react';
import { Link2, Network, Puzzle, RefreshCcw, Server, UserRound, Users } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import type { Role } from '../../api/client';
import { AccountSettings } from './AccountSettings';
import { FleetSettings } from './FleetSettings';
import { ModulesSettings } from './ModulesSettings';
import { NodeSettings } from './NodeSettings';
import { PairingSettings } from './PairingSettings';
import { UpdatesSettings } from './UpdatesSettings';
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
  // Anyone may see what this node is; only an administrator renames it or changes its role.
  { id: 'node', label: 'Node', icon: Server, minRole: null, component: NodeSettings },
  // Anyone may see which nodes are paired; only an administrator pairs and unpairs.
  { id: 'pairing', label: 'Pairing', icon: Link2, minRole: null, component: PairingSettings },
  // Every paired node asked now: how it is reached and what it runs (ADR-012 §2.6).
  { id: 'fleet', label: 'Fleet', icon: Network, minRole: null, component: FleetSettings },
  { id: 'users', label: 'Users', icon: Users, minRole: 'admin', component: UsersSettings },
  // Anyone may see the versions; only an administrator changes them (the section says so).
  { id: 'updates', label: 'Updates', icon: RefreshCcw, minRole: null, component: UpdatesSettings },
  // Anyone may see the modules; only an administrator changes them.
  { id: 'modules', label: 'Modules', icon: Puzzle, minRole: null, component: ModulesSettings },
];
