import { useCallback, useEffect, useMemo, useRef } from "react";
import { useParams } from "react-router-dom";
import useSWR from "swr";
import {
  ReactFlow,
  Controls,
  MiniMap,
  Background,
  useNodesState,
  useEdgesState,
  addEdge,
  type Node,
  type Edge,
  type Connection,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { toast } from "sonner";

import { apiPost, apiDelete, swrFetcher } from "../lib/api";
import { ApiRequestError } from "../lib/api";
import { connectEvents, type EventConnection } from "../lib/ws";
import type { TaskStatusChanged, DagMutated } from "../lib/types";
import TaskNode from "../components/TaskNode";
import DeletableEdge from "../components/DeletableEdge";
import WaveTimeline from "../components/WaveTimeline";

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
  critical_path?: string[];
}

interface DagDetailResponse {
  nodes: Array<{ id: string; title: string; status: string; domain?: string }>;
  edges: Array<{ from: string; to: string }>;
  critical_path: string[];
}

const nodeTypes = { task: TaskNode };
const edgeTypes = { deletable: DeletableEdge };

export default function DagView() {
  const { id } = useParams<{ id: string }>();

  // Fetch basic DAG layout
  const { data, mutate } = useSWR<DagResponse>(
    id ? `/dag?epic_id=${id}` : null,
    swrFetcher,
  );

  // Fetch critical path from detail endpoint
  const { data: detailData } = useSWR<DagDetailResponse>(
    id ? `/dag/${id}` : null,
    swrFetcher,
  );

  const criticalPathSet = useMemo(() => {
    if (!detailData?.critical_path) return new Set<string>();
    const cp = detailData.critical_path;
    const edgeSet = new Set<string>();
    for (let i = 0; i < cp.length - 1; i++) {
      edgeSet.add(`${cp[i]}-${cp[i + 1]}`);
    }
    return edgeSet;
  }, [detailData]);

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  // Compute layer info for WaveTimeline from node positions
  const waveNodes = useMemo(() => {
    if (!data?.nodes) return [];
    // Group by x position to determine layers
    const xValues = [...new Set(data.nodes.map((n) => n.x))].sort(
      (a, b) => a - b,
    );
    const xToLayer = new Map(xValues.map((x, i) => [x, i]));
    return data.nodes.map((n) => ({
      id: n.id,
      layer: xToLayer.get(n.x) ?? 0,
      status: n.status,
    }));
  }, [data]);

  // Edge delete handler passed into edge data
  const handleEdgeDelete = useCallback(
    async (edgeId: string) => {
      const [from, ...rest] = edgeId.split("-");
      const to = rest.join("-");

      // Optimistic remove
      setEdges((eds) => eds.filter((e) => e.id !== edgeId));

      try {
        await apiDelete(`/deps/${from}/${to}`);
        mutate();
      } catch {
        // Rollback
        mutate();
        toast.error("Failed to remove dependency");
      }
    },
    [mutate, setEdges],
  );

  // Convert API response to ReactFlow format
  useEffect(() => {
    if (!data) return;
    setNodes(
      data.nodes.map((n) => ({
        id: n.id,
        type: "task",
        position: { x: n.x, y: n.y },
        data: {
          title: n.title,
          status: n.status,
          domain: n.domain,
          estimated_seconds: n.estimated_seconds,
        },
      })),
    );
    setEdges(
      data.edges.map((e) => {
        const edgeId = `${e.from}-${e.to}`;
        const isCritical = criticalPathSet.has(edgeId);
        return {
          id: edgeId,
          source: e.from,
          target: e.to,
          type: "deletable",
          animated: isCritical,
          data: {
            isCritical,
            onDelete: handleEdgeDelete,
          },
        };
      }),
    );
  }, [data, criticalPathSet, setNodes, setEdges, handleEdgeDelete]);

  // Drag-to-create dependency
  const onConnect = useCallback(
    async (connection: Connection) => {
      // Self-loop prevention
      if (connection.source === connection.target) return;

      const edgeId = `${connection.source}-${connection.target}`;

      // Optimistic add
      setEdges((eds) =>
        addEdge(
          {
            ...connection,
            id: edgeId,
            type: "deletable",
            animated: false,
            data: { isCritical: false, onDelete: handleEdgeDelete },
          },
          eds,
        ),
      );

      try {
        await apiPost("/deps", {
          from: connection.source,
          to: connection.target,
        });
        mutate();
      } catch (err) {
        // Rollback the optimistic edge
        setEdges((eds) => eds.filter((e) => e.id !== edgeId));
        if (err instanceof ApiRequestError && err.status === 409) {
          toast.error("Cyclic dependency detected");
        } else if (
          err instanceof ApiRequestError &&
          err.serverMessage.includes("cycle")
        ) {
          toast.error("Cyclic dependency detected");
        } else {
          toast.error("Failed to create dependency");
        }
      }
    },
    [mutate, setEdges, handleEdgeDelete],
  );

  // WebSocket for real-time updates
  const connRef = useRef<EventConnection | null>(null);

  useEffect(() => {
    if (!id) return;

    const conn = connectEvents();
    connRef.current = conn;

    conn.on<TaskStatusChanged>("TaskStatusChanged", (evt) => {
      if (evt.epic_id === id) {
        // Update node status in-place for instant feedback
        setNodes((nds) =>
          nds.map((n) => {
            if (n.id === evt.task_id) {
              return {
                ...n,
                data: { ...n.data, status: evt.to_status },
              };
            }
            return n;
          }),
        );
        // Also refetch for full consistency
        mutate();
      }
    });

    conn.on<DagMutated>("DagMutated", () => {
      mutate();
    });

    return () => {
      conn.close();
      connRef.current = null;
    };
  }, [id, mutate, setNodes]);

  const proOptions = useMemo(() => ({ hideAttribution: true }), []);

  return (
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-bold mb-4">DAG: {id}</h1>
      <div className="flex-1 rounded-lg border border-border-primary overflow-hidden min-h-[500px]">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          proOptions={proOptions}
          fitView
        >
          <Controls />
          <MiniMap />
          <Background />
        </ReactFlow>
      </div>
      <WaveTimeline nodes={waveNodes} />
    </div>
  );
}
