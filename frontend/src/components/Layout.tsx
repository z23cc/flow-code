import { useEffect, useState, useCallback } from "react";
import { NavLink, Link, Outlet, useLocation, useNavigate } from "react-router-dom";
import {
  LayoutDashboard,
  Bot,
  Brain,
  Settings,
  ChevronRight,
  Menu,
  X,
} from "lucide-react";
import useSWR from "swr";
import { swrFetcher } from "../lib/api";

interface Epic {
  id: string;
  status: string;
}

const navItems = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard, shortcut: "D" },
  { to: "/agents", label: "Agents", icon: Bot, shortcut: "A" },
  { to: "/memory", label: "Memory", icon: Brain, shortcut: "M" },
  { to: "/settings", label: "Settings", icon: Settings, shortcut: "S" },
];

function breadcrumbFromPath(pathname: string): { label: string; to?: string }[] {
  if (pathname === "/") return [{ label: "Dashboard" }];
  const segments = pathname.split("/").filter(Boolean);
  const crumbs: { label: string; to?: string }[] = [
    { label: "Dashboard", to: "/" },
  ];

  if (segments[0] === "epic" && segments[1]) {
    crumbs.push({ label: `Epic: ${segments[1]}` });
  } else if (segments[0] === "dag" && segments[1]) {
    crumbs.push({ label: `Epic: ${segments[1]}`, to: `/epic/${segments[1]}` });
    crumbs.push({ label: "DAG" });
  } else if (segments[0] === "replay" && segments[1]) {
    crumbs.push({ label: `Replay: ${segments[1]}` });
  } else {
    crumbs.push({
      label: segments[0].charAt(0).toUpperCase() + segments[0].slice(1),
    });
  }

  return crumbs;
}

export default function Layout() {
  const location = useLocation();
  const navigate = useNavigate();
  const crumbs = breadcrumbFromPath(location.pathname);
  const [sidebarOpen, setSidebarOpen] = useState(false);

  // Fetch epics to find the first open one for the G shortcut
  const { data: epics } = useSWR<Epic[]>("/epics", swrFetcher);

  const firstOpenEpicId =
    epics?.find((e) => e.status === "open" || e.status === "active")?.id ??
    epics?.[0]?.id;

  // Keyboard shortcuts
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      // Skip when focus is in an input, textarea, or contenteditable
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      ) {
        return;
      }

      // Skip if modifier keys are held
      if (e.ctrlKey || e.metaKey || e.altKey) return;

      switch (e.key.toLowerCase()) {
        case "d":
          e.preventDefault();
          navigate("/");
          break;
        case "g":
          e.preventDefault();
          if (firstOpenEpicId) {
            navigate(`/dag/${firstOpenEpicId}`);
          }
          break;
        case "a":
          e.preventDefault();
          navigate("/agents");
          break;
        case "m":
          e.preventDefault();
          navigate("/memory");
          break;
        case "s":
          e.preventDefault();
          navigate("/settings");
          break;
      }
    },
    [navigate, firstOpenEpicId],
  );

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  // Close sidebar on navigation (mobile)
  useEffect(() => {
    setSidebarOpen(false);
  }, [location.pathname]);

  return (
    <div className="flex h-screen bg-bg-primary text-text-primary">
      {/* Mobile overlay */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 bg-black/50 z-30 md:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      {/* Sidebar */}
      <aside
        className={`fixed inset-y-0 left-0 z-40 w-56 border-r border-border bg-bg-secondary flex flex-col transition-transform duration-200 md:static md:translate-x-0 ${
          sidebarOpen ? "translate-x-0" : "-translate-x-full"
        }`}
      >
        <div className="h-14 flex items-center justify-between px-4 border-b border-border">
          <span className="text-accent font-bold text-lg tracking-tight">
            Flow Code
          </span>
          <button
            className="md:hidden p-1 rounded hover:bg-bg-tertiary"
            onClick={() => setSidebarOpen(false)}
          >
            <X size={18} />
          </button>
        </div>
        <nav className="flex-1 py-2 space-y-0.5 px-2">
          {navItems.map(({ to, label, icon: Icon, shortcut }) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              className={({ isActive }) =>
                `flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
                  isActive
                    ? "bg-accent/10 text-accent"
                    : "text-text-secondary hover:bg-bg-tertiary hover:text-text-primary"
                }`
              }
            >
              <Icon size={18} />
              <span className="flex-1">{label}</span>
              <kbd className="hidden md:inline-block text-[10px] font-mono px-1.5 py-0.5 rounded bg-bg-tertiary text-text-muted">
                {shortcut}
              </kbd>
            </NavLink>
          ))}
        </nav>
        <div className="px-4 py-3 border-t border-border text-xs text-text-muted">
          v0.1.0
        </div>
      </aside>

      {/* Main content */}
      <div className="flex-1 flex flex-col overflow-hidden min-w-0">
        {/* Breadcrumb header */}
        <header className="h-14 flex items-center px-4 md:px-6 border-b border-border bg-bg-secondary/50 shrink-0 gap-3">
          {/* Mobile hamburger */}
          <button
            className="md:hidden p-1 rounded hover:bg-bg-tertiary"
            onClick={() => setSidebarOpen(true)}
          >
            <Menu size={20} />
          </button>

          <nav className="flex items-center gap-1 text-sm text-text-secondary min-w-0">
            {crumbs.map((crumb, i) => (
              <span key={i} className="flex items-center gap-1 min-w-0">
                {i > 0 && (
                  <ChevronRight size={14} className="text-text-muted shrink-0" />
                )}
                {crumb.to && i < crumbs.length - 1 ? (
                  <Link
                    to={crumb.to}
                    className="hover:text-text-primary truncate"
                  >
                    {crumb.label}
                  </Link>
                ) : (
                  <span
                    className={`truncate ${
                      i === crumbs.length - 1 ? "text-text-primary" : ""
                    }`}
                  >
                    {crumb.label}
                  </span>
                )}
              </span>
            ))}
          </nav>

          {/* Keyboard shortcut hint (desktop) */}
          <div className="hidden md:flex items-center gap-1 ml-auto text-[10px] text-text-muted">
            <kbd className="font-mono px-1 py-0.5 rounded bg-bg-tertiary">G</kbd>
            <span>DAG</span>
          </div>
        </header>

        {/* Page content */}
        <main className="flex-1 overflow-auto p-4 md:p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
