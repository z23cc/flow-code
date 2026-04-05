import { useParams } from "react-router-dom";

export default function Replay() {
  const { id } = useParams();
  return (
    <div>
      <h1 className="text-2xl font-bold mb-4">Replay: {id}</h1>
      <p className="text-text-secondary">Task execution replay and timeline.</p>
    </div>
  );
}
