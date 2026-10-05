// Test helpers for the course detail screen (imported only by *.test.tsx files).

import { screen } from "@testing-library/react";
import { toast } from "sonner";
import { paths } from "@/lib/routes";
import { type RenderRouteOptions, renderRoute } from "@/test/render";

/** Course ids from src/api/mock/fixtures.ts. */
export const DEMO101 = "folder:demo-courses/course/DEMO101"; // week 4, learning aid, no URL
export const DEMO205 = "canvas:canvas.demo.test/course/205"; // Canvas, policy not set, has URL
export const DEMO310 = "folder:demo-courses/course/DEMO310"; // no term dates → week unknown
export const DEMO099 = "canvas:canvas.demo.test/course/99"; // hidden, no deadlines
/** Only in the scenario "canvas-hidden-lists": no Pages and no Files list in Canvas. */
export const DEMO312 = "canvas:canvas.demo.test/course/312";

/** Render /courses/<id>[?query] and wait until the course (its tab list) has loaded. */
export async function openCourse(
  courseId: string,
  options: RenderRouteOptions & { query?: string } = {},
) {
  const { query, ...rest } = options;
  // Sonner keeps toasts in a module-level store and replays them to every new <Toaster/>, so
  // a toast from an earlier test could otherwise satisfy this test's findByText.
  toast.dismiss();
  const result = renderRoute(`${paths.course(courseId)}${query ? `?${query}` : ""}`, rest);
  await screen.findByRole("tablist", { name: "Course sections" });
  return result;
}
