import { useCallback, useEffect, useMemo } from "react";
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

import { apiFetch, swrFetcher } from "../lib/api";
import TaskNode from "../components/TaskNode";

interface DagNode {
  id: string;
  title: string;
  status: string;
  x: number;
  y: number;
  domain?: string;
}

interface DagEdge {
  from: string;
  to: string;
}

interface DagResponse {
  nodes: DagNode[];
  edges: DagEdge[];
  version?: string;
}

const nodeTypes = { task: TaskNode };

export default function DagView() {
  const { id } = useParams<{ id: string }>();
  const { data, mutate } = useSWR<DagResponse>(
    id ? `/dag?epic_id=${id}` : null,
    swrFetcher,
  );

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  // Convert API response to ReactFlow format
  useEffect(() => {
    if (!data) return;
    setNodes(
      data.nodes.map((n) => ({
        id: n.id,
        type: "task",
        position: { x: n.x, y: n.y },
        data: { title: n.title, status: n.status, domain: n.domain },
      })),
    );
    setEdges(
      data.edges.map((e) => ({
        id: `${e.from}-${e.to}`,
        source: e.from,
        target: e.to,
        animated: true,
      })),
    );
  }, [data, setNodes, setEdges]);

  // Add dependency on connect
  const onConnect = useCallback(
    async (connection: Connection) => {
      setEdges((eds) => addEdge({ ...connection, animated: true }, eds));
      await apiFetch("/dag/mutate", {
        method: "POST",
        body: JSON.stringify({
          action: "add_dep",
          params: { task_id: connection.target, dep_id: connection.source },
          version: data?.version,
        }),
      });
      mutate();
    },
    [data?.version, mutate, setEdges],
  );

  // Remove dependency on edge delete
  const onEdgesDelete = useCallback(
    async (deleted: Edge[]) => {
      for (const edge of deleted) {
        await apiFetch("/dag/mutate", {
          method: "POST",
          body: JSON.stringify({
            action: "remove_dep",
            params: { task_id: edge.target, dep_id: edge.source },
            version: data?.version,
          }),
        });
      }
      mutate();
    },
    [data?.version, mutate],
  );

  // WebSocket for live updates
  useEffect(() => {
    if (!id) return;
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(`${protocol}//${window.location.host}/api/v1/events`);
    ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data);
        if (msg.type === "task_status_change") {
          mutate();
        }
      } catch {
        // ignore non-JSON messages
      }
    };
    return () => ws.close();
  }, [id, mutate]);

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
          onEdgesDelete={onEdgesDelete}
          nodeTypes={nodeTypes}
          proOptions={proOptions}
          fitView
        >
          <Controls />
          <MiniMap />
          <Background />
        </ReactFlow>
      </div>
    </div>
  );
}
