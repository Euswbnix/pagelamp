import { BookOpenText, Cable, FolderSync, Settings } from "lucide-react";
import { useTranslation } from "react-i18next";
import { NavLink } from "react-router";
import { BrandMark } from "@/components/common/BrandMark";
import { paths } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { SyncPill } from "./SyncPill";

const NAV = [
  { to: paths.courses, icon: BookOpenText, key: "nav.courses" },
  { to: paths.sources, icon: FolderSync, key: "nav.sources" },
  { to: paths.connect, icon: Cable, key: "nav.connect" },
  { to: paths.settings, icon: Settings, key: "nav.settings" },
] as const;

/** Left navigation. Plain links, so Tab/Enter work and the active page gets aria-current. */
export function Sidebar() {
  const { t } = useTranslation();
  return (
    <aside className="flex w-60 shrink-0 flex-col border-r bg-sidebar text-sidebar-foreground">
      <div className="px-4 pt-5 pb-4">
        <BrandMark />
      </div>
      <nav aria-label={t("nav.label")} className="flex-1 px-2">
        <ul className="space-y-0.5">
          {NAV.map(({ to, icon: Icon, key }) => (
            <li key={to}>
              <NavLink
                to={to}
                className={({ isActive }) =>
                  cn(
                    "flex items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors",
                    "hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
                    "focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-sidebar-ring",
                    isActive && "bg-sidebar-accent font-medium text-sidebar-accent-foreground",
                  )
                }
              >
                <Icon className="size-4 shrink-0" aria-hidden />
                {t(key)}
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>
      <div className="border-t p-2">
        <SyncPill />
      </div>
    </aside>
  );
}
