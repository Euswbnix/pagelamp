// Provisional until B5: StoreCounts.removed_courses (calendar design §8.5), which needs the
// tombstones table (schema v4). Every other removal and dates-v2 type is generated now
// (backend-2's F2 contract, 5874258). Delete this file when `pnpm gen:types` brings the field.

declare module "../generated" {
  interface StoreCounts {
    /** Removed courses (pending and purged) listed under "Removed courses". */
    removed_courses: number;
  }
}

export {};
