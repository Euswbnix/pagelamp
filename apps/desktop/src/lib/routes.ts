// URL helpers. Course ids contain ':' and '/' (e.g. "canvas:canvas.example.edu/course/42"),
// so they are always encoded as one path segment.

export const paths = {
  welcome: "/welcome",
  courses: "/courses",
  course: (courseId: string) => `/courses/${encodeURIComponent(courseId)}`,
  sources: "/sources",
  connect: "/connect",
  settings: "/settings",
  plan: "/plan",
} as const;
