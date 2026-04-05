import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
}

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false };

  static getDerivedStateFromError(): State {
    return { hasError: true };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("ErrorBoundary caught:", error, info.componentStack);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="flex flex-col items-center justify-center h-screen gap-4 text-text-primary">
          <h1 className="text-xl font-semibold">Something went wrong</h1>
          <p className="text-sm text-text-muted">
            An unexpected error occurred.
          </p>
          <button
            className="px-4 py-2 rounded-md bg-accent text-white text-sm hover:opacity-90 min-h-[44px] min-w-[44px]"
            onClick={() => this.setState({ hasError: false })}
          >
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
