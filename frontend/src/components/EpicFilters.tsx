import { Search, X } from "lucide-react";

export interface EpicFiltersValue {
  search: string;
  status: string;
}

export const DEFAULT_EPIC_FILTERS: EpicFiltersValue = {
  search: "",
  status: "all",
};

interface EpicFiltersProps {
  value: EpicFiltersValue;
  onChange: (next: EpicFiltersValue) => void;
  statusOptions: string[];
  filteredCount: number;
  totalCount: number;
}

export default function EpicFilters({
  value,
  onChange,
  statusOptions,
  filteredCount,
  totalCount,
}: EpicFiltersProps) {
  const active = value.search.trim() !== "" || value.status !== "all";

  return (
    <div className="flex flex-col sm:flex-row sm:items-center gap-3 mb-3">
      <div className="relative flex-1 min-w-0">
        <Search
          size={14}
          className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none"
        />
        <input
          type="text"
          value={value.search}
          onChange={(e) => onChange({ ...value, search: e.target.value })}
          placeholder="Search epics by id or title…"
          className="w-full pl-8 pr-8 py-2 rounded-md text-sm bg-bg-secondary border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent transition-colors"
        />
        {value.search && (
          <button
            onClick={() => onChange({ ...value, search: "" })}
            className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-primary transition-colors"
            aria-label="Clear search"
          >
            <X size={14} />
          </button>
        )}
      </div>

      <select
        value={value.status}
        onChange={(e) => onChange({ ...value, status: e.target.value })}
        className="px-3 py-2 rounded-md text-sm bg-bg-secondary border border-border text-text-primary focus:outline-none focus:border-accent transition-colors"
      >
        <option value="all">All statuses</option>
        {statusOptions.map((s) => (
          <option key={s} value={s}>
            {s.replace("_", " ")}
          </option>
        ))}
      </select>

      {active && (
        <div className="flex items-center gap-2 text-xs text-text-muted">
          <span className="font-mono whitespace-nowrap">
            {filteredCount} / {totalCount}
          </span>
          <button
            onClick={() => onChange(DEFAULT_EPIC_FILTERS)}
            className="text-accent hover:text-accent-hover transition-colors whitespace-nowrap"
          >
            Clear filters
          </button>
        </div>
      )}
    </div>
  );
}
