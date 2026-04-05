import useSWR from "swr";
import { swrFetcher } from "../lib/api";

export default function Settings() {
  const { data, error, isLoading } = useSWR<Record<string, unknown>>(
    "/config",
    swrFetcher,
  );

  const entries = data ? Object.entries(data) : [];

  return (
    <div className="flex flex-col h-full gap-4">
      <h1 className="text-2xl font-bold">Settings</h1>

      {isLoading && <p className="text-text-muted text-sm">Loading...</p>}
      {error && (
        <p className="text-error text-sm">Failed to load configuration.</p>
      )}

      {!isLoading && entries.length === 0 && (
        <div className="flex-1 flex items-center justify-center">
          <p className="text-text-muted text-sm">No configuration found.</p>
        </div>
      )}

      {entries.length > 0 && (
        <div className="rounded-md border border-border overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="bg-bg-tertiary text-text-secondary text-left">
                <th className="px-4 py-2 font-medium">Key</th>
                <th className="px-4 py-2 font-medium">Value</th>
              </tr>
            </thead>
            <tbody>
              {entries.map(([key, value]) => (
                <tr
                  key={key}
                  className="border-t border-border bg-bg-secondary align-top"
                >
                  <td className="px-4 py-3 font-mono text-accent whitespace-nowrap">
                    {key}
                  </td>
                  <td className="px-4 py-3 text-text-primary">
                    {typeof value === "object" && value !== null ? (
                      <div className="flex flex-col gap-1 text-xs">
                        {Object.entries(value as Record<string, unknown>).map(
                          ([k, v]) => (
                            <div key={k} className="flex gap-2">
                              <span className="text-text-muted font-mono">
                                {k}:
                              </span>
                              <span className="font-mono">
                                {typeof v === "object"
                                  ? JSON.stringify(v)
                                  : String(v)}
                              </span>
                            </div>
                          ),
                        )}
                      </div>
                    ) : (
                      <span className="font-mono text-xs">{String(value)}</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
