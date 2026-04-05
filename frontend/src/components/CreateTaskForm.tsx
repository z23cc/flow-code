import { useState, type FormEvent } from "react";
import { X } from "lucide-react";

const DOMAINS = [
  "general",
  "frontend",
  "backend",
  "architecture",
  "testing",
  "docs",
  "ops",
];

interface CreateTaskFormProps {
  epicId: string;
  open: boolean;
  onClose: () => void;
  onCreated: () => void;
}

export default function CreateTaskForm({
  epicId,
  open,
  onClose,
  onCreated,
}: CreateTaskFormProps) {
  const [title, setTitle] = useState("");
  const [deps, setDeps] = useState("");
  const [domain, setDomain] = useState("general");
  const [submitting, setSubmitting] = useState(false);

  if (!open) return null;

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!title.trim()) return;
    setSubmitting(true);
    try {
      const body: Record<string, unknown> = {
        epic_id: epicId,
        title: title.trim(),
        domain,
      };
      const depList = deps
        .split(",")
        .map((d) => d.trim())
        .filter(Boolean);
      if (depList.length > 0) body.depends_on = depList;

      const res = await fetch("/api/v1/tasks/create", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      if (!res.ok) throw new Error("Failed to create task");
      setTitle("");
      setDeps("");
      setDomain("general");
      onCreated();
      onClose();
    } finally {
      setSubmitting(false);
    }
  }

  const inputCls =
    "w-full rounded-md border border-border bg-bg-primary px-3 py-2 text-sm text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none";

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-full max-w-md rounded-lg border border-border bg-bg-secondary p-6 shadow-xl">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold">Create Task</h2>
          <button
            onClick={onClose}
            className="text-text-muted hover:text-text-primary"
          >
            <X size={18} />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm text-text-secondary mb-1">
              Title <span className="text-error">*</span>
            </label>
            <input
              className={inputCls}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Task title"
              required
            />
          </div>

          <div>
            <label className="block text-sm text-text-secondary mb-1">
              Dependencies
            </label>
            <input
              className={inputCls}
              value={deps}
              onChange={(e) => setDeps(e.target.value)}
              placeholder="task-1, task-2"
            />
            <p className="mt-1 text-xs text-text-muted">
              Comma-separated task IDs
            </p>
          </div>

          <div>
            <label className="block text-sm text-text-secondary mb-1">
              Domain
            </label>
            <select
              className={inputCls}
              value={domain}
              onChange={(e) => setDomain(e.target.value)}
            >
              {DOMAINS.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </select>
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 rounded-md text-sm text-text-secondary hover:bg-bg-tertiary"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting || !title.trim()}
              className="px-4 py-2 rounded-md text-sm font-medium bg-accent text-bg-primary hover:bg-accent-hover disabled:opacity-50"
            >
              {submitting ? "Creating..." : "Create"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
