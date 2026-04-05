import type { ReactNode } from "react";

interface CardProps {
  children: ReactNode;
  header?: ReactNode;
  footer?: ReactNode;
  className?: string;
}

export default function Card({ children, header, footer, className = "" }: CardProps) {
  return (
    <div className={`rounded-[var(--radius-md)] border border-border bg-bg-secondary ${className}`}>
      {header && (
        <div className="border-b border-border px-5 py-3">{header}</div>
      )}
      <div className="p-5">{children}</div>
      {footer && (
        <div className="border-t border-border px-5 py-3">{footer}</div>
      )}
    </div>
  );
}
