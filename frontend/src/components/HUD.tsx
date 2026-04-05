import { useEffect, useState, useRef } from "react";
import { connectEvents, type EventConnection } from "../lib/ws";
import type { TaskStatusChanged } from "../lib/types";
import useSWR from "swr";
import { swrFetcher } from "../lib/api";

interface Stats {
  total_tasks: number;
  done_tasks: number;
  in_progress_tasks: number;
  blocked_tasks: number;
}

export default function HUD() {
  const { data: stats } = useSWR<Stats>("/stats", swrFetcher, {
    refreshInterval: 30_000,
  });

  const [counts, setCounts] = useState({
    done: 0,
    total: 0,
    active: 0,
    blocked: 0,
  });

  // Sync from SWR data
  useEffect(() => {
    if (stats) {
      setCounts({
        done: stats.done_tasks,
        total: stats.total_tasks,
        active: stats.in_progress_tasks,
        blocked: stats.blocked_tasks,
      });
    }
  }, [stats]);

  // Subscribe to WS for live updates
  const connRef = useRef<EventConnection | null>(null);
  useEffect(() => {
    const conn = connectEvents();
    connRef.current = conn;

    conn.on<TaskStatusChanged>("TaskStatusChanged", (data) => {
      setCounts((prev) => {
        const next = { ...prev };
        // Decrement old status
        if (data.from_status === "done") next.done--;
        else if (data.from_status === "in_progress") next.active--;
        else if (data.from_status === "blocked") next.blocked--;
        // Increment new status
        if (data.to_status === "done") next.done++;
        else if (data.to_status === "in_progress") next.active++;
        else if (data.to_status === "blocked") next.blocked++;
        return next;
      });
    });

    return () => conn.close();
  }, []);

  return (
    <div aria-live="polite" className="h-8 flex items-center px-4 md:px-6 border-b border-border bg-bg-secondary/30 text-xs font-mono shrink-0 gap-4">
      <span className="text-text-secondary">
        <span className="text-success font-medium">{counts.done}</span>
        <span className="text-text-muted">/{counts.total}</span>
        {" done"}
      </span>
      <span className="text-text-muted">|</span>
      <span className="text-text-secondary">
        <span className="text-accent font-medium">{counts.active}</span>
        {" active"}
      </span>
      <span className="text-text-muted">|</span>
      <span className="text-text-secondary">
        <span className={counts.blocked > 0 ? "text-error font-medium" : "text-text-muted font-medium"}>
          {counts.blocked}
        </span>
        {" blocked"}
      </span>
    </div>
  );
}
