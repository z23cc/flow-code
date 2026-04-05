import { createContext, useContext, useState, useCallback, type ReactNode } from "react";

interface TaskSidebarState {
  isOpen: boolean;
  taskId: string | null;
  open: (id: string) => void;
  close: () => void;
}

const TaskSidebarContext = createContext<TaskSidebarState>({
  isOpen: false,
  taskId: null,
  open: () => {},
  close: () => {},
});

export function TaskSidebarProvider({ children }: { children: ReactNode }) {
  const [taskId, setTaskId] = useState<string | null>(null);
  const [isOpen, setIsOpen] = useState(false);

  const open = useCallback((id: string) => {
    setTaskId(id);
    setIsOpen(true);
  }, []);

  const close = useCallback(() => {
    setIsOpen(false);
  }, []);

  return (
    <TaskSidebarContext.Provider value={{ isOpen, taskId, open, close }}>
      {children}
    </TaskSidebarContext.Provider>
  );
}

export function useTaskSidebar() {
  return useContext(TaskSidebarContext);
}
