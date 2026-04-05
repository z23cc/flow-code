import { useEffect } from "react";
import { Routes, Route } from "react-router-dom";
import { Toaster } from "sonner";
import Layout from "./components/Layout";
import Dashboard from "./pages/Dashboard";
import EpicDetail from "./pages/EpicDetail";
import DagView from "./pages/DagView";
import Agents from "./pages/Agents";
import Memory from "./pages/Memory";
import Settings from "./pages/Settings";
import Replay from "./pages/Replay";
import CommandPalette from "./components/CommandPalette";
import TaskSidebar from "./components/TaskSidebar";
import { TaskSidebarProvider } from "./components/TaskSidebarContext";
import { startToastBridge } from "./lib/toast-bridge";

export default function App() {
  useEffect(() => {
    const cleanup = startToastBridge();
    return cleanup;
  }, []);

  return (
    <TaskSidebarProvider>
      <Toaster
        theme="dark"
        position="bottom-right"
        toastOptions={{
          style: {
            background: "var(--color-bg-secondary)",
            border: "1px solid var(--color-border)",
            color: "var(--color-text-primary)",
          },
        }}
      />
      <CommandPalette />
      <TaskSidebar />
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<Dashboard />} />
          <Route path="/epic/:id" element={<EpicDetail />} />
          <Route path="/dag/:id" element={<DagView />} />
          <Route path="/agents" element={<Agents />} />
          <Route path="/memory" element={<Memory />} />
          <Route path="/settings" element={<Settings />} />
          <Route path="/replay/:id" element={<Replay />} />
        </Route>
      </Routes>
    </TaskSidebarProvider>
  );
}
