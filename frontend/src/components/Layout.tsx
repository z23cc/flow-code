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
import { connectEvents, type ConnectionState } from "../lib/ws";
import HUD from "./HUD";

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

function WsIndicator() {
  const [wsState, setWsState] = useState<ConnectionState>("disconnected");

  useEffect(() => {
    const conn = connectEvents();
    conn.onConnectionChange(setWsState);
    setWsState(conn.state);
    return () => conn.close();
  }, []);

  const color =
    wsState === "connected"
      ? "bg-success"
      : wsState === "reconnecting"
        ? "bg-warning animate-pulse"
        : "bg-error";

  const label =
    wsState === "connected"
      ? "Connected"
      : wsState === "reconnecting"
        ? "Reconnecting"
        : "Disconnected";

  return (
    <span className="flex items-center gap-1.5" title={label}>
      <span className={`inline-block w-2 h-2 rounded-full ${color}`} />
      <span className="hidden sm:inline text-[10px] text-text-muted">{label}</span>
    </span>
  );
}

export default function Layout() {
  const location = useLocation();
  const navigate = useNavigate();
  const crumbs = breadcrumbFromPath(location.pathname);
  const [sidebarOpen, setSidebarOpen] = useState(false);

  const { data: epics } = useSWR<Epic[]>("/epics", swrFetcher);

  const firstOpenEpicId =
    epics?.find((e) => e.status === "open" || e.status === "active")?.id ??
    epics?.[0]?.id;

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      ) {
        return;
      }

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

  useEffect(() => {
    setSidebarOpen(false);
  }, [location.pathname]);

  return (
    <div className="flex h-screen bg-bg-primary text-text-primary">
      {/* Mobile overlay */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 bg-black/50 z-30 lg:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      {/* Sidebar - hidden below 768px (bottom tabs instead), icon-only 768-1024px, full above 1024px */}
      <aside
        role="complementary"
        aria-label="Sidebar navigation"
        className={`fixed inset-y-0 left-0 z-40 border-r border-border bg-bg-secondary flex-col transition-all duration-200
          hidden md:flex
          md:static md:translate-x-0
          md:w-[56px] lg:w-[200px]
          ${sidebarOpen ? "!flex translate-x-0 w-56" : "-translate-x-full"}`}
      >
        <div className="h-14 flex items-center justify-between px-3 lg:px-4 border-b border-border">
          <span className="text-accent font-bold text-lg tracking-tight hidden lg:block">
            Flow Code
          </span>
          <span className="text-accent font-bold text-lg tracking-tight lg:hidden">
            FC
          </span>
          <button
            className="lg:hidden p-1 rounded hover:bg-bg-tertiary min-h-[44px] min-w-[44px] flex items-center justify-center"
            onClick={() => setSidebarOpen(false)}
            aria-label="Close sidebar"
          >
            <X size={18} />
          </button>
        </div>
        <nav role="navigation" aria-label="Main navigation" className="flex-1 py-2 space-y-0.5 px-1 lg:px-2">
          {navItems.map(({ to, label, icon: Icon, shortcut }) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              tabIndex={0}
              className={({ isActive }) =>
                `flex items-center gap-3 px-2 lg:px-3 py-2 rounded-md text-sm transition-colors min-h-[44px] ${
                  isActive
                    ? "bg-accent/10 text-accent"
                    : "text-text-secondary hover:bg-bg-tertiary hover:text-text-primary"
                }`
              }
            >
              <Icon size={18} className="shrink-0" />
              <span className="flex-1 hidden lg:block">{label}</span>
              <kbd className="hidden lg:inline-block text-[10px] font-mono px-1.5 py-0.5 rounded bg-bg-tertiary text-text-muted">
                {shortcut}
              </kbd>
            </NavLink>
          ))}
        </nav>
        <div className="px-4 py-3 border-t border-border text-xs text-text-muted hidden lg:block">
          v0.1.0
        </div>
      </aside>

      {/* Main content */}
      <div className="flex-1 flex flex-col overflow-hidden min-w-0">
        {/* Breadcrumb header */}
        <header className="h-14 flex items-center px-4 md:px-6 border-b border-border bg-bg-secondary/50 shrink-0 gap-3">
          {/* Mobile hamburger */}
          <button
            className="md:hidden p-1 rounded hover:bg-bg-tertiary min-h-[44px] min-w-[44px] flex items-center justify-center"
            onClick={() => setSidebarOpen(true)}
            aria-label="Open sidebar"
          >
            <Menu size={20} />
          </button>

          <nav aria-label="Breadcrumb" className="flex items-center gap-1 text-sm text-text-secondary min-w-0">
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

          {/* WS indicator + shortcuts (desktop) */}
          <div className="flex items-center gap-3 ml-auto">
            <WsIndicator />
            <div className="hidden md:flex items-center gap-2 text-[10px] text-text-muted">
              <span className="flex items-center gap-1">
                <kbd className="font-mono px-1 py-0.5 rounded bg-bg-tertiary">G</kbd>
                <span>DAG</span>
              </span>
              <span className="flex items-center gap-1">
                <kbd className="font-mono px-1 py-0.5 rounded bg-bg-tertiary">{"\u2318"}K</kbd>
                <span>Commands</span>
              </span>
            </div>
          </div>
        </header>

        {/* HUD status bar */}
        <HUD />

        {/* Page content */}
        <main role="main" className="flex-1 overflow-auto p-4 md:p-6 pb-20 md:pb-6">
          <Outlet />
        </main>
      </div>

      {/* Bottom tab navigation for mobile (≤768px) */}
      <nav
        role="navigation"
        aria-label="Mobile navigation"
        className="fixed bottom-0 left-0 right-0 z-40 md:hidden border-t border-border bg-bg-secondary flex items-center justify-around h-14"
      >
        {navItems.map(({ to, label, icon: Icon }) => (
          <NavLink
            key={to}
            to={to}
            end={to === "/"}
            className={({ isActive }) =>
              `flex flex-col items-center justify-center gap-0.5 min-h-[44px] min-w-[44px] px-2 py-1 text-[10px] transition-colors ${
                isActive
                  ? "text-accent"
                  : "text-text-muted"
              }`
            }
          >
            <Icon size={20} />
            <span>{label}</span>
          </NavLink>
        ))}
      </nav>
    </div>
  );
}
