import { useEffect, useRef } from "react";
import { Outlet, useLocation } from "react-router";
import { useRefreshOnWindowFocus } from "@/api/queries";
import { brand } from "@/brand";
import { CrashNotice } from "@/features/diagnostics/CrashNotice";
import { PostUpdateBanner } from "@/features/updates/PostUpdateBanner";
import { UpdateNotice } from "@/features/updates/UpdateNotice";
import { useUpdateLifecycle } from "@/features/updates/useUpdateLifecycle";
import { WhatsNewSheet } from "@/features/updates/WhatsNewSheet";
import { useRefreshAfterExternalSync } from "@/stores/sync";
import { Sidebar } from "./Sidebar";

/** Sidebar + scrollable content area. Every screen except onboarding renders inside this. */
export function AppShell() {
  const mainRef = useRef<HTMLElement>(null);
  useRouteAnnouncements(mainRef);
  useRefreshAfterExternalSync();
  useRefreshOnWindowFocus();
  useUpdateLifecycle();
  return (
    <div className="flex h-dvh overflow-hidden">
      <Sidebar />
      <main
        ref={mainRef}
        id="main"
        tabIndex={-1}
        className="pl-content min-w-0 flex-1 overflow-y-auto outline-none"
      >
        <div className="mx-auto max-w-5xl px-8 py-8">
          <CrashNotice />
          <PostUpdateBanner />
          <UpdateNotice />
          <WhatsNewSheet />
          <Outlet />
        </div>
      </main>
    </div>
  );
}

/**
 * After a navigation (not on first load): if the click that navigated left focus nowhere, move
 * it to the page so keyboard users continue from the top, and name the window after the page's
 * heading once it has rendered.
 */
function useRouteAnnouncements(mainRef: React.RefObject<HTMLElement | null>) {
  const { pathname } = useLocation();
  const lastPath = useRef<string | null>(null);
  useEffect(() => {
    const main = mainRef.current;
    if (!main) return;
    const navigated = lastPath.current !== null && lastPath.current !== pathname;
    lastPath.current = pathname;
    if (navigated && (document.activeElement === document.body || !document.activeElement)) {
      main.focus({ preventScroll: true });
    }
    main.scrollTop = 0;
    // The h1 may appear after data loads; watch for it and keep the title in sync.
    const update = () => {
      const heading = main.querySelector("h1")?.textContent?.trim();
      document.title = heading ? `${heading} – ${brand.productName}` : brand.productName;
    };
    update();
    const observer = new MutationObserver(update);
    observer.observe(main, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [pathname, mainRef]);
}
