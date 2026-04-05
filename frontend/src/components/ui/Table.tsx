import { type ReactNode, useState } from "react";
import { ChevronUp, ChevronDown } from "lucide-react";

interface Column<T> {
  key: string;
  header: string;
  render: (row: T) => ReactNode;
  sortable?: boolean;
  sortValue?: (row: T) => string | number;
}

interface TableProps<T> {
  columns: Column<T>[];
  data: T[];
  keyExtractor: (row: T) => string;
  className?: string;
}

export default function Table<T>({ columns, data, keyExtractor, className = "" }: TableProps<T>) {
  const [sortKey, setSortKey] = useState<string | null>(null);
  const [sortAsc, setSortAsc] = useState(true);

  const handleSort = (col: Column<T>) => {
    if (!col.sortable) return;
    if (sortKey === col.key) {
      setSortAsc(!sortAsc);
    } else {
      setSortKey(col.key);
      setSortAsc(true);
    }
  };

  const sorted = sortKey
    ? [...data].sort((a, b) => {
        const col = columns.find((c) => c.key === sortKey);
        if (!col?.sortValue) return 0;
        const av = col.sortValue(a);
        const bv = col.sortValue(b);
        const cmp = av < bv ? -1 : av > bv ? 1 : 0;
        return sortAsc ? cmp : -cmp;
      })
    : data;

  return (
    <div className={`overflow-x-auto ${className}`}>
      <table className="responsive-table w-full border-collapse text-sm">
        <thead>
          <tr className="border-b border-border">
            {columns.map((col) => (
              <th
                key={col.key}
                className={`px-4 py-3 text-left font-medium text-text-secondary ${col.sortable ? "cursor-pointer select-none hover:text-text-primary" : ""}`}
                onClick={() => handleSort(col)}
              >
                <span className="inline-flex items-center gap-1">
                  {col.header}
                  {col.sortable && sortKey === col.key && (
                    sortAsc ? <ChevronUp size={14} /> : <ChevronDown size={14} />
                  )}
                </span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((row) => (
            <tr key={keyExtractor(row)} className="border-b border-border transition-colors hover:bg-bg-tertiary/50">
              {columns.map((col) => (
                <td key={col.key} className="px-4 py-3 text-text-primary" data-label={col.header}>
                  {col.render(row)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
