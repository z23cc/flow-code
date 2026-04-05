import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import useSWR from "swr";
import {
  ReactFlow,
  Controls,
  MiniMap,
  Background,
  useNodesState,
  useEdgesState,
  type Node,
  type Edge,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Play, Pause, RotateCcw } from "lucide-react";

import { swrFetcher } from "../lib/api";
import TaskNode from "../components/TaskNode";

/* ── API types ────────────────────────────────────────────── */

interface Epic {
  id: string;
  title: string;
  status: string;
}

interface Task {
  id: string;
  title: string;
  status: string;
  domain?: string;
  depends_on?: string[];
  duration_seconds?: number;
  created_at?: string;
  updated_at?: string;
  estimated_seconds?: number | null;
}

interface DagNode {
  id: string;
  title: string;
  status: string;
  x: number;
  y: number;
  domain?: string;
  estimated_seconds?: number | null;
}

interface DagEdge {
  from: string;
  to: string;
}

interface DagResponse {
  nodes: DagNode[];
  edges: DagEdge[];
}

/* ── Timeline event ───────────────────────────────────────── */

interface TimelineEvent {
  time: number; // seconds from epoch 0 of this replay
  taskId: string;
  event: "start" | "complete";
}

/* ── Helpers ──────────────────────────────────────────────── */

const SPEEDS = [1, 2, 5, 10] as const;

function formatTime(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

/**
 * Build a sorted timeline of start/complete events from tasks.
 *
 * We reconstruct timing from available data:
 * - Tasks with status "done" completed at updated_at and started
 *   duration_seconds earlier (or 30s default).
 * - Tasks with status "in_progress" started at updated_at.
 * - Tasks with status "todo" / "blocked" are not included.
 *
 * All times are normalised so the earliest event is t=0.
 */
function buildTimeline(tasks: Task[]): {
  events: TimelineEvent[];
  totalDuration: number;
} {
  const raw: { time: number; taskId: string; event: "start" | "complete" }[] =
    [];

  for (const t of tasks) {
    if (t.status === "done") {
      const completedMs = t.updated_at ? new Date(t.updated_at).getTime() : 0;
      if (completedMs === 0) continue;
      const dur = (t.duration_seconds ?? 30) * 1000;
      const startedMs = completedMs - dur;
      raw.push({ time: startedMs, taskId: t.id, event: "start" });
      raw.push({ time: completedMs, taskId: t.id, event: "complete" });
    } else if (t.status === "in_progress") {
      const startedMs = t.updated_at ? new Date(t.updated_at).getTime() : 0;
      if (startedMs === 0) continue;
      raw.push({ time: startedMs, taskId: t.id, event: "start" });
    }
  }

  if (raw.length === 0) return { events: [], totalDuration: 0 };

  raw.sort((a, b) => a.time - b.time);
  const origin = raw[0].time;
  const events: TimelineEvent[] = raw.map((r) => ({
    time: (r.time - origin) / 1000, // convert to seconds from origin
    taskId: r.taskId,
    event: r.event,
  }));
  const totalDuration = events[events.length - 1].time;

  return { events, totalDuration: Math.max(totalDuration, 1) };
}

/* ── Component ────────────────────────────────────────────── */

const nodeTypes = { task: TaskNode };

export default function Replay() {
  const { id: paramId } = useParams<{ id: string }>();
  const navigate = useNavigate();

  // Epic selector state
  const [selectedEpicId, setSelectedEpicId] = useState<string | null>(
    paramId ?? null,
  );

  const { data: epics } = useSWR<Epic[]>("/epics", swrFetcher);

  // Fetch tasks for the selected epic
  const { data: tasks } = useSWR<Task[]>(
    selectedEpicId ? `/tasks?epic_id=${selectedEpicId}` : null,
    swrFetcher,
  );

  // Fetch DAG structure
  const { data: dagData } = useSWR<DagResponse>(
    selectedEpicId ? `/dag?epic_id=${selectedEpicId}` : null,
    swrFetcher,
  );

  // Playback state
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState<(typeof SPEEDS)[number]>(1);
  const [currentTime, setCurrentTime] = useState(0);
  const animRef = useRef<number | null>(null);
  const lastFrameRef = useRef<number | null>(null);

  // ReactFlow state
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges] = useEdgesState<Edge>([]);

  // Build timeline from tasks
  const { events, totalDuration } = useMemo(() => {
    if (!tasks || tasks.length === 0) return { events: [], totalDuration: 0 };
    return buildTimeline(tasks);
  }, [tasks]);

  // Initialise DAG nodes (all gray = todo)
  useEffect(() => {
    if (!dagData) return;
    setNodes(
      dagData.nodes.map((n) => ({
        id: n.id,
        type: "task",
        position: { x: n.x, y: n.y },
        data: {
          title: n.title,
          status: "todo",
          domain: n.domain,
          estimated_seconds: n.estimated_seconds,
        },
      })),
    );
    setEdges(
      dagData.edges.map((e) => ({
        id: `${e.from}-${e.to}`,
        source: e.from,
        target: e.to,
        animated: false,
      })),
    );
    // Reset playback when epic changes
    setCurrentTime(0);
    setPlaying(false);
    lastFrameRef.current = null;
  }, [dagData, setNodes, setEdges]);

  // Compute node statuses from current playback time
  const updateNodeStatuses = useCallback(
    (time: number) => {
      // Build status map: track latest event per task up to `time`
      const statusMap = new Map<string, string>();
      for (const evt of events) {
        if (evt.time > time) break;
        if (evt.event === "start") statusMap.set(evt.taskId, "in_progress");
        else statusMap.set(evt.taskId, "done");
      }

      setNodes((nds) =>
        nds.map((n) => {
          const newStatus = statusMap.get(n.id) ?? "todo";
          if ((n.data as { status: string }).status === newStatus) return n;
          return {
            ...n,
            data: { ...n.data, status: newStatus },
          };
        }),
      );
    },
    [events, setNodes],
  );

  // Animation loop
  useEffect(() => {
    if (!playing) {
      if (animRef.current != null) {
        cancelAnimationFrame(animRef.current);
        animRef.current = null;
      }
      lastFrameRef.current = null;
      return;
    }

    const tick = (timestamp: number) => {
      if (lastFrameRef.current == null) {
        lastFrameRef.current = timestamp;
      }
      const delta = ((timestamp - lastFrameRef.current) / 1000) * speed;
      lastFrameRef.current = timestamp;

      setCurrentTime((prev) => {
        const next = prev + delta;
        if (next >= totalDuration) {
          setPlaying(false);
          updateNodeStatuses(totalDuration);
          return totalDuration;
        }
        updateNodeStatuses(next);
        return next;
      });

      animRef.current = requestAnimationFrame(tick);
    };

    animRef.current = requestAnimationFrame(tick);
    return () => {
      if (animRef.current != null) cancelAnimationFrame(animRef.current);
    };
  }, [playing, speed, totalDuration, updateNodeStatuses]);

  // Scrub handler
  const handleScrub = (e: React.ChangeEvent<HTMLInputElement>) => {
    const t = parseFloat(e.target.value);
    setCurrentTime(t);
    updateNodeStatuses(t);
    // Pause on manual scrub
    setPlaying(false);
    lastFrameRef.current = null;
  };

  // Reset
  const handleReset = () => {
    setPlaying(false);
    setCurrentTime(0);
    lastFrameRef.current = null;
    updateNodeStatuses(0);
  };

  // Epic change
  const handleEpicChange = (epicId: string) => {
    setSelectedEpicId(epicId);
    navigate(`/replay/${epicId}`, { replace: true });
  };

  const proOptions = useMemo(() => ({ hideAttribution: true }), []);

  const hasTasks = tasks && tasks.length > 0;
  const hasEvents = events.length > 0;

  return (
    <div className="flex flex-col h-full gap-4">
      {/* Header row: title + epic selector */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <h1 className="text-2xl font-bold">Replay</h1>
        <select
          value={selectedEpicId ?? ""}
          onChange={(e) => handleEpicChange(e.target.value)}
          className="rounded-md border border-border bg-bg-secondary px-3 py-2 text-sm text-text-primary"
        >
          <option value="" disabled>
            Select an epic...
          </option>
          {epics?.map((ep) => (
            <option key={ep.id} value={ep.id}>
              {ep.title} ({ep.status})
            </option>
          ))}
        </select>
      </div>

      {/* DAG canvas */}
      {!selectedEpicId && (
        <div className="flex-1 flex items-center justify-center text-text-muted">
          Select an epic to replay its execution.
        </div>
      )}

      {selectedEpicId && !hasTasks && tasks !== undefined && (
        <div className="flex-1 flex items-center justify-center text-text-muted">
          No tasks to replay.
        </div>
      )}

      {selectedEpicId && hasTasks && (
        <>
          <div className="flex-1 rounded-lg border border-border-primary overflow-hidden min-h-[400px]">
            <ReactFlow
              nodes={nodes}
              edges={edges}
              onNodesChange={onNodesChange}
              nodeTypes={nodeTypes}
              proOptions={proOptions}
              nodesDraggable={false}
              nodesConnectable={false}
              fitView
            >
              <Controls />
              <MiniMap />
              <Background />
            </ReactFlow>
          </div>

          {/* Playback controls */}
          <div className="rounded-lg border border-border bg-bg-secondary p-4">
            <div className="flex items-center gap-4">
              {/* Play / Pause */}
              <button
                onClick={() => {
                  if (currentTime >= totalDuration) {
                    handleReset();
                    setPlaying(true);
                  } else {
                    setPlaying((p) => !p);
                  }
                }}
                disabled={!hasEvents}
                className="flex items-center justify-center w-9 h-9 rounded-md bg-accent text-bg-primary hover:bg-accent-hover transition-colors disabled:opacity-50"
              >
                {playing ? <Pause size={18} /> : <Play size={18} />}
              </button>

              {/* Reset */}
              <button
                onClick={handleReset}
                disabled={!hasEvents}
                className="flex items-center justify-center w-9 h-9 rounded-md border border-border hover:bg-bg-tertiary transition-colors disabled:opacity-50"
              >
                <RotateCcw size={16} />
              </button>

              {/* Time display */}
              <span className="text-sm font-mono text-text-secondary min-w-[100px]">
                {formatTime(currentTime)} / {formatTime(totalDuration)}
              </span>

              {/* Scrub bar */}
              <input
                type="range"
                min={0}
                max={totalDuration}
                step={0.1}
                value={currentTime}
                onChange={handleScrub}
                disabled={!hasEvents}
                className="flex-1 h-2 accent-accent"
              />

              {/* Speed selector */}
              <div className="flex items-center gap-1">
                {SPEEDS.map((s) => (
                  <button
                    key={s}
                    onClick={() => setSpeed(s)}
                    className={`px-2 py-1 rounded text-xs font-medium transition-colors ${
                      speed === s
                        ? "bg-accent text-bg-primary"
                        : "bg-bg-tertiary text-text-secondary hover:text-text-primary"
                    }`}
                  >
                    {s}x
                  </button>
                ))}
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
