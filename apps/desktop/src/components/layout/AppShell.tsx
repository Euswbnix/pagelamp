import { Outlet } from "react-router";
import { Sidebar } from "./Sidebar";

/** Sidebar + scrollable content area. Every screen except onboarding renders inside this. */
export function AppShell() {
  return (
    <div className="flex h-dvh overflow-hidden">
      <Sidebar />
      <main id="main" className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-5xl px-8 py-8">
          <Outlet />
        </div>
      </main>
    </div>
  );
}
