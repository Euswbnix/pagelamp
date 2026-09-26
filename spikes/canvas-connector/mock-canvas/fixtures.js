// Synthetic, obviously-fake Canvas data for the spike. No real people, courses or institutions.
import crypto from 'node:crypto';

// Test login for the LOCAL mock only. Not a real credential for anything.
export const TEST_LOGIN = Object.freeze({
  unique_id: 'student@example.test',
  password: 'mock-canvas-password',
});

export const USER = Object.freeze({
  id: 1001,
  name: 'Ada Example',
  created_at: '2025-08-20T12:00:00Z',
  sortable_name: 'Example, Ada',
  short_name: 'Ada',
  avatar_url: 'https://example.test/avatar.png',
  locale: null,
  effective_locale: 'en-CA',
  permissions: { can_update_name: false, can_update_avatar: true },
});

// 8 courses so that a server-side per_page clamp of 3 forces 3 pages (3 + 3 + 2).
export const COURSES = Array.from({ length: 8 }, (_, i) => {
  const id = 2001 + i;
  return {
    id,
    name: `Synthetic Course ${String.fromCharCode(65 + i)}`,
    course_code: `SYN${100 + i}`,
    workflow_state: 'available',
    account_id: 1,
    start_at: '2026-09-02T04:00:00Z',
    end_at: '2026-12-20T05:00:00Z',
    enrollment_term_id: 77,
    enrollments: [{ type: 'student', role: 'StudentEnrollment', enrollment_state: 'active', user_id: USER.id }],
    time_zone: 'America/Toronto',
    default_view: 'modules',
  };
});

// Deterministic pseudo-random bytes (so hashes are stable across runs).
export function deterministicBytes(seed, size) {
  const out = Buffer.allocUnsafe(size);
  let off = 0;
  let counter = 0;
  while (off < size) {
    const block = crypto.createHash('sha256').update(`${seed}:${counter++}`).digest();
    off += block.copy(out, off, 0, Math.min(block.length, size - off));
  }
  return out;
}

export const FILES = (() => {
  const small = Buffer.concat([Buffer.from('%PDF-1.4\n% synthetic spike file\n'), deterministicBytes('small', 24 * 1024)]);
  const large = deterministicBytes('large', 5 * 1024 * 1024);
  const mk = (id, courseId, display, ctype, bytes) => ({
    id,
    courseId,
    display_name: display,
    filename: display.replace(/\s+/g, '_'),
    'content-type': ctype,
    size: bytes.length,
    bytes,
    sha256: crypto.createHash('sha256').update(bytes).digest('hex'),
    uuid: crypto.createHash('md5').update(`uuid-${id}`).digest('hex'),
  });
  return new Map([
    [5001, mk(5001, 2001, 'week1 notes.pdf', 'application/pdf', small)],
    [5002, mk(5002, 2001, 'lecture slides large.bin', 'application/octet-stream', large)],
  ]);
})();

export function modulesFor(courseId) {
  const base = (courseId - 2000) * 100;
  const mods = [
    {
      id: base + 1,
      name: 'Week 1 - Introduction',
      position: 1,
      unlock_at: null,
      require_sequential_progress: false,
      state: 'completed',
      items_count: 3,
    },
    {
      id: base + 2,
      name: 'Week 2 - Foundations',
      position: 2,
      unlock_at: null,
      require_sequential_progress: false,
      state: 'unlocked',
      items_count: 2,
    },
  ];
  const items = {
    [base + 1]: [
      { id: base * 10 + 1, title: 'Course overview', type: 'Page', page_url: 'course-overview', position: 1, indent: 0 },
      { id: base * 10 + 2, title: 'Problem Set 1', type: 'Assignment', content_id: base * 10 + 900, position: 2, indent: 0 },
      ...(courseId === 2001
        ? [
            { id: base * 10 + 3, title: 'week1 notes.pdf', type: 'File', content_id: 5001, position: 3, indent: 1 },
            { id: base * 10 + 4, title: 'lecture slides large.bin', type: 'File', content_id: 5002, position: 4, indent: 1 },
          ]
        : [{ id: base * 10 + 3, title: 'Reading list', type: 'ExternalUrl', external_url: 'https://example.test/reading', position: 3, indent: 0 }]),
    ],
    [base + 2]: [
      { id: base * 10 + 5, title: 'Foundations notes', type: 'Page', page_url: 'foundations', position: 1, indent: 0 },
      { id: base * 10 + 6, title: 'Quiz 1', type: 'Quiz', content_id: base * 10 + 901, position: 2, indent: 0 },
    ],
  };
  return { mods, items };
}

export function plannerItems() {
  return COURSES.slice(0, 4).flatMap((c, i) => [
    {
      context_type: 'Course',
      course_id: c.id,
      plannable_id: c.id * 10 + 900,
      plannable_type: 'assignment',
      plannable_date: `2026-10-0${i + 1}T03:59:59Z`,
      plannable: { id: c.id * 10 + 900, title: `Problem Set 1 (${c.course_code})`, due_at: `2026-10-0${i + 1}T03:59:59Z`, points_possible: 10 },
      submissions: { submitted: false, missing: false, late: false, graded: false },
      html_url: `/courses/${c.id}/assignments/${c.id * 10 + 900}`,
      context_name: c.name,
    },
    {
      context_type: 'Course',
      course_id: c.id,
      plannable_id: c.id * 10 + 901,
      plannable_type: 'quiz',
      plannable_date: `2026-10-1${i}T03:59:59Z`,
      plannable: { id: c.id * 10 + 901, title: `Quiz 1 (${c.course_code})`, due_at: `2026-10-1${i}T03:59:59Z` },
      submissions: false,
      html_url: `/courses/${c.id}/quizzes/${c.id * 10 + 901}`,
      context_name: c.name,
    },
  ]);
}
