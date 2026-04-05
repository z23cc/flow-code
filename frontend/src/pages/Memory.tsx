import { useState } from "react";
import useSWR from "swr";
import { swrFetcher } from "../lib/api";

interface MemoryEntry {
  id: string;
  type: string;
  content: string;
  module: string;
  severity: string;
  track: string;
  tags: string[];
}

interface MemoryResponse {
  entries: MemoryEntry[];
}

function typeBadge(type: string): { color: string; label: string } {
  switch (type) {
    case "pitfall":
      return { color: "bg-error/20 text-error", label: "Pitfall" };
    case "convention":
      return { color: "bg-info/20 text-info", label: "Convention" };
    case "decision":
      return { color: "bg-purple-500/20 text-purple-400", label: "Decision" };
    default:
      return { color: "bg-bg-tertiary text-text-muted", label: type };
  }
}

export default function Memory() {
  const [trackFilter, setTrackFilter] = useState("all");
  const [search, setSearch] = useState("");

  const { data, error, isLoading } = useSWR<MemoryResponse>(
    "/memory",
    swrFetcher,
  );

  const entries = (data?.entries ?? []).filter((e) => {
    if (trackFilter !== "all" && e.track !== trackFilter) return false;
    if (search && !e.content.toLowerCase().includes(search.toLowerCase()))
      return false;
    return true;
  });

  return (
    <div className="flex flex-col h-full gap-4">
      <h1 className="text-2xl font-bold">Memory</h1>

      <div className="flex gap-3">
        <select
          value={trackFilter}
          onChange={(e) => setTrackFilter(e.target.value)}
          className="px-3 py-2 rounded-md bg-bg-secondary border border-border text-sm text-text-primary focus:outline-none focus:border-border-accent"
        >
          <option value="all">All</option>
          <option value="bug">Bug</option>
          <option value="knowledge">Knowledge</option>
        </select>

        <input
          type="text"
          placeholder="Search content..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="flex-1 px-3 py-2 rounded-md bg-bg-secondary border border-border text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-border-accent"
        />
      </div>

      {isLoading && <p className="text-text-muted text-sm">Loading...</p>}
      {error && (
        <p className="text-error text-sm">
          Failed to load memory entries.
        </p>
      )}

      {!isLoading && entries.length === 0 && (
        <div className="flex-1 flex flex-col items-center justify-center gap-3 text-center">
          <div className="text-4xl opacity-40">🧠</div>
          <p className="text-text-primary text-sm font-medium">
            No memory entries yet
          </p>
          <p className="text-text-muted text-xs max-w-md">
            Memory accumulates automatically as agents work. Lessons from
            completed tasks, pitfalls, and conventions will appear here.
          </p>
        </div>
      )}

      <div className="flex-1 overflow-auto space-y-3">
        {entries.map((entry) => {
          const badge = typeBadge(entry.type);
          return (
            <div
              key={entry.id}
              className="rounded-md border border-border bg-bg-secondary p-4 space-y-2"
            >
              <div className="flex items-center gap-2 flex-wrap">
                <span
                  className={`px-2 py-0.5 rounded text-xs font-medium ${badge.color}`}
                >
                  {badge.label}
                </span>
                <span className="px-2 py-0.5 rounded text-xs font-medium bg-accent/20 text-accent">
                  {entry.track}
                </span>
                {entry.module && (
                  <span className="px-2 py-0.5 rounded text-xs font-medium bg-bg-tertiary text-text-secondary">
                    {entry.module}
                  </span>
                )}
              </div>
              <p className="text-sm text-text-primary leading-relaxed">
                {entry.content}
              </p>
              {entry.tags.length > 0 && (
                <div className="flex gap-1 flex-wrap">
                  {entry.tags.map((tag) => (
                    <span
                      key={tag}
                      className="text-xs text-text-muted"
                    >
                      #{tag}
                    </span>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
