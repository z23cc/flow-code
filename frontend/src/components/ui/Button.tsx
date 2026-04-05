import { type ButtonHTMLAttributes } from "react";
import { Loader2 } from "lucide-react";

const variants = {
  primary: "bg-accent text-white hover:bg-accent-hover",
  secondary: "bg-bg-tertiary text-text-primary hover:bg-border",
  ghost: "bg-transparent text-text-secondary hover:bg-bg-tertiary",
  danger: "bg-error text-white hover:opacity-80",
} as const;

const sizes = {
  sm: "px-2.5 py-1 text-xs",
  md: "px-4 py-2 text-sm",
  lg: "px-6 py-3 text-base",
} as const;

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: keyof typeof variants;
  size?: keyof typeof sizes;
  loading?: boolean;
}

export default function Button({
  variant = "primary",
  size = "md",
  loading = false,
  disabled,
  className = "",
  children,
  ...props
}: ButtonProps) {
  return (
    <button
      className={`inline-flex items-center justify-center gap-2 rounded-md font-medium transition-all duration-[var(--transition-default)] disabled:opacity-50 disabled:cursor-not-allowed ${variants[variant]} ${sizes[size]} ${className}`}
      disabled={disabled || loading}
      {...props}
    >
      {loading && <Loader2 size={16} className="animate-spin" />}
      {children}
    </button>
  );
}
