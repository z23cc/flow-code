import {
  BaseEdge,
  EdgeLabelRenderer,
  getBezierPath,
  type EdgeProps,
} from "@xyflow/react";

interface DeletableEdgeData {
  isCritical?: boolean;
  onDelete?: (id: string) => void;
}

export default function DeletableEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  markerEnd,
}: EdgeProps) {
  const edgeData = data as DeletableEdgeData | undefined;
  const isCritical = edgeData?.isCritical ?? false;
  const onDelete = edgeData?.onDelete;

  const [edgePath, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  });

  return (
    <>
      <BaseEdge
        id={id}
        path={edgePath}
        markerEnd={markerEnd}
        style={{
          stroke: isCritical ? "var(--color-status-blocked)" : "#555",
          strokeWidth: isCritical ? 3 : 1.5,
        }}
      />
      {onDelete && (
        <EdgeLabelRenderer>
          <button
            className="edge-delete-btn"
            style={{
              position: "absolute",
              transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
              pointerEvents: "all",
            }}
            onClick={() => onDelete(id)}
            title="Remove dependency"
          >
            &times;
          </button>
        </EdgeLabelRenderer>
      )}
    </>
  );
}
