import { type ReactNode, useEffect, useRef } from "react";
import { X } from "lucide-react";

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: ReactNode;
  className?: string;
}

export default function Modal({ open, onClose, title, children, className = "" }: ModalProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const el = dialogRef.current;
    if (!el) return;
    if (open && !el.open) el.showModal();
    else if (!open && el.open) el.close();
  }, [open]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && open) onClose();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <dialog
      ref={dialogRef}
      onClose={onClose}
      className="fixed inset-0 z-50 m-0 h-full w-full bg-transparent p-0 backdrop:bg-black/60 backdrop:backdrop-blur-sm"
    >
      <div className="flex h-full w-full items-center justify-center p-4" onClick={onClose}>
        <div
          className={`w-full max-w-lg rounded-[var(--radius-md)] border border-border bg-bg-secondary shadow-xl ${className}`}
          onClick={(e) => e.stopPropagation()}
        >
          {title && (
            <div className="flex items-center justify-between border-b border-border px-5 py-3">
              <h2 className="text-lg font-semibold text-text-primary">{title}</h2>
              <button onClick={onClose} className="text-text-muted hover:text-text-primary transition-colors">
                <X size={18} />
              </button>
            </div>
          )}
          <div className="p-5">{children}</div>
        </div>
      </div>
    </dialog>
  );
}
