// URL helpers. Course ids contain ':' and '/' (e.g. "canvas:canvas.example.edu/course/42"),
// so they are always encoded as one path segment.

export const paths = {
  welcome: "/welcome",
  courses: "/courses",
  course: (courseId: string) => `/courses/${encodeURIComponent(courseId)}`,
  /** The course's page, which then opens "What PageLamp didn't read" and scrolls to it. */
  courseNotRead: (courseId: string) => `/courses/${encodeURIComponent(courseId)}?show=not-read`,
  sources: "/sources",
  connect: "/connect",
  settings: "/settings",
} as const;
