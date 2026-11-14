// The session log shown in the inspector's Info tab: things that happened in this workbench.

import { create } from 'zustand';

export type LogLevel = 'info' | 'warn' | 'error';

export interface LogEntry {
  id: number;
  atMs: number;
  level: LogLevel;
  text: string;
}

const MAX_ENTRIES = 500;
let nextId = 1;

export interface LogState {
  entries: LogEntry[];
  add: (level: LogLevel, text: string) => void;
}

export const useLog = create<LogState>((set) => ({
  entries: [],
  add: (level, text) =>
    set((state) => ({
      entries: [...state.entries, { id: nextId++, atMs: Date.now(), level, text }].slice(
        -MAX_ENTRIES
      ),
    })),
}));

export const log = {
  info: (text: string) => useLog.getState().add('info', text),
  warn: (text: string) => useLog.getState().add('warn', text),
  error: (text: string) => useLog.getState().add('error', text),
};
