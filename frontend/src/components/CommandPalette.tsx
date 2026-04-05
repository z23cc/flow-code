import { useState, useEffect, useCallback, useRef } from "react";
import { useNavigate } from "react-router-dom";
import useSWR from "swr";
import {
  Search,
  Play,
  Ban,
  RotateCcw,
  SkipForward,
  LayoutDashboard,
  Bot,
  Brain,
  Settings,
  Plus,
  type LucideIcon,
} from "lucide-react";
import { toast } from "sonner";
import { apiPost, swrFetcher } from "../lib/api";

interface Task {
  id: string;
  title: string;
  status: string;
}

interface TasksResponse {
  count: number;
  tasks: Task[];
}

interface Command {
  id: string;
  label: string;
  description: string;
  icon: LucideIcon;
  action: () => void | Promise<void>;
}

export default function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();

  const { data: tasksData } = useSWR<TasksResponse>(
    open ? "/tasks" : null,
    swrFetcher,
  );

  const tasks = tasksData?.tasks ?? [];

  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
    setSelectedIndex(0);
  }, []);

  const execute = useCallback(async (cmd: Command) => {
    close();
    try {
      await cmd.action();
    } catch (err) {
      toast.error(`Command failed: ${err instanceof Error ? err.message : "unknown error"}`);
    }
  }, [close]);

  // Build command list
  const commands: Command[] = [];

  // Task commands
  for (const task of tasks) {
    if (task.status === "todo" || task.status === "blocked") {
      commands.push({
        id: `start-${task.id}`,
        label: `start ${task.id}`,
        description: task.title,
        icon: Play,
        action: async () => {
          await apiPost(`/tasks/${task.id}/start`, {});
          toast.success(`Started ${task.id}`);
        },
      });
    }
    if (task.status === "in_progress") {
      commands.push({
        id: `block-${task.id}`,
        label: `block ${task.id}`,
        description: task.title,
        icon: Ban,
        action: async () => {
          await apiPost(`/tasks/${task.id}/block`, {});
          toast.success(`Blocked ${task.id}`);
        },
      });
    }
    if (task.status === "blocked" || task.status === "failed") {
      commands.push({
        id: `restart-${task.id}`,
        label: `restart ${task.id}`,
        description: task.title,
        icon: RotateCcw,
        action: async () => {
          await apiPost(`/tasks/${task.id}/restart`, {});
          toast.success(`Restarted ${task.id}`);
        },
      });
    }
    if (task.status !== "done" && task.status !== "skipped") {
      commands.push({
        id: `skip-${task.id}`,
        label: `skip ${task.id}`,
        description: task.title,
        icon: SkipForward,
        action: async () => {
          await apiPost(`/tasks/${task.id}/skip`, {});
          toast.success(`Skipped ${task.id}`);
        },
      });
    }
  }

  // Navigation commands
  commands.push(
    {
      id: "go-dashboard",
      label: "go Dashboard",
      description: "Navigate to Dashboard",
      icon: LayoutDashboard,
      action: () => navigate("/"),
    },
    {
      id: "go-agents",
      label: "go Agents",
      description: "Navigate to Agents",
      icon: Bot,
      action: () => navigate("/agents"),
    },
    {
      id: "go-memory",
      label: "go Memory",
      description: "Navigate to Memory",
      icon: Brain,
      action: () => navigate("/memory"),
    },
    {
      id: "go-settings",
      label: "go Settings",
      description: "Navigate to Settings",
      icon: Settings,
      action: () => navigate("/settings"),
    },
    {
      id: "create-epic",
      label: "create epic",
      description: "Navigate to create a new epic",
      icon: Plus,
      action: () => navigate("/"),
    },
  );

  // Filter
  const q = query.toLowerCase().trim();
  const filtered = q
    ? commands.filter(
        (cmd) =>
          cmd.label.toLowerCase().includes(q) ||
          cmd.description.toLowerCase().includes(q),
      )
    : commands;

  // Clamp selection
  const clampedIndex = Math.min(selectedIndex, Math.max(filtered.length - 1, 0));

  // Keyboard shortcut to open
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setOpen((prev) => !prev);
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  // Focus input when opened
  useEffect(() => {
    if (open) {
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  // Scroll selected item into view
  useEffect(() => {
    if (!listRef.current) return;
    const item = listRef.current.children[clampedIndex] as HTMLElement | undefined;
    item?.scrollIntoView({ block: "nearest" });
  }, [clampedIndex]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
        break;
      case "Enter":
        e.preventDefault();
        if (filtered[clampedIndex]) {
          execute(filtered[clampedIndex]);
        }
        break;
      case "Escape":
        e.preventDefault();
        close();
        break;
    }
  };

  // Reset selection on query change
  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[60] flex items-start justify-center pt-[20vh] bg-black/50 backdrop-blur-sm"
      onClick={close}
    >
      <div
        className="w-full max-w-[600px] mx-4 rounded-[var(--radius-md)] border border-border bg-bg-secondary shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search input */}
        <div className="flex items-center gap-3 px-4 border-b border-border">
          <Search size={16} className="text-text-muted shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Type a command..."
            className="flex-1 bg-transparent py-3 text-sm text-text-primary placeholder:text-text-muted outline-none"
          />
          <kbd className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-bg-tertiary text-text-muted shrink-0">
            ESC
          </kbd>
        </div>

        {/* Results */}
        <div ref={listRef} className="max-h-[300px] overflow-auto py-1">
          {filtered.length === 0 ? (
            <div className="px-4 py-6 text-center text-sm text-text-muted">
              No matching commands
            </div>
          ) : (
            filtered.map((cmd, i) => {
              const Icon = cmd.icon;
              const isSelected = i === clampedIndex;
              return (
                <button
                  key={cmd.id}
                  className={`w-full flex items-center gap-3 px-4 py-2.5 text-left transition-colors cursor-pointer ${
                    isSelected
                      ? "bg-accent/10 text-accent"
                      : "text-text-secondary hover:bg-bg-tertiary"
                  }`}
                  onClick={() => execute(cmd)}
                  onMouseEnter={() => setSelectedIndex(i)}
                >
                  <Icon size={16} className="shrink-0" />
                  <div className="min-w-0 flex-1">
                    <span className="text-sm font-medium">{cmd.label}</span>
                    <span className="ml-2 text-xs text-text-muted">{cmd.description}</span>
                  </div>
                </button>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
