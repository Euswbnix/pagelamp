// The one-time question (b) reminders (D37 option 2) that readings said and the student hasn't
// answered or dismissed yet. The facade says each course's only once, so a reminder outlives
// the screen whose reading said it: the course's page shows it, and so does the batch card
// that read it. In memory only, like a toast: never persisted, and it holds no course content.

import { create } from "zustand";

export interface PendingReminder {
  /** Where the course's text went. */
  service: string;
  /** Said during "Read syllabi for N courses": the batch card shows it too. */
  batch: boolean;
}

interface SharingRemindersState {
  /** By course id, in the order they were said. */
  pending: Record<string, PendingReminder>;
  add: (courseId: string, reminder: PendingReminder) => void;
  /** Answered or dismissed (anywhere): gone everywhere. */
  settle: (courseId: string) => void;
  reset: () => void;
}

export const useSharingReminders = create<SharingRemindersState>()((set) => ({
  pending: {},
  add: (courseId, reminder) =>
    set((s) => ({
      pending: {
        ...s.pending,
        [courseId]: { ...reminder, batch: reminder.batch || !!s.pending[courseId]?.batch },
      },
    })),
  settle: (courseId) =>
    set((s) => {
      if (!(courseId in s.pending)) return s;
      const pending = { ...s.pending };
      delete pending[courseId];
      return { pending };
    }),
  reset: () => set({ pending: {} }),
}));
