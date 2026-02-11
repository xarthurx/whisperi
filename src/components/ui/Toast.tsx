import * as React from "react";
import { X, CheckCircle2, AlertCircle, Info } from "lucide-react";
import { cn } from "@/utils/cn";

export interface ToastProps {
  id?: string;
  title?: string;
  description?: string;
  action?: React.ReactNode;
  variant?: "default" | "destructive" | "success";
  duration?: number;
  onClose?: () => void;
}

export interface ToastContextType {
  toast: (props: Omit<ToastProps, "id">) => void;
  dismiss: (id?: string) => void;
  toastCount: number;
}

const ToastContext = React.createContext<ToastContextType | undefined>(undefined);

export const useToast = () => {
  const context = React.useContext(ToastContext);
  if (!context) {
    throw new Error("useToast must be used within a ToastProvider");
  }
  return context;
};

interface ToastState extends ToastProps {
  id: string;
  isExiting?: boolean;
  createdAt: number;
}

export const ToastProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [toasts, setToasts] = React.useState<ToastState[]>([]);
  const timersRef = React.useRef<Record<string, ReturnType<typeof setTimeout>>>({});

  const clearTimer = React.useCallback((id: string) => {
    const timer = timersRef.current[id];
    if (timer) {
      clearTimeout(timer);
      delete timersRef.current[id];
    }
  }, []);

  const startExitAnimation = React.useCallback((id: string) => {
    setToasts((prev) => prev.map((t) => (t.id === id ? { ...t, isExiting: true } : t)));
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 200);
  }, []);

  const toast = React.useCallback(
    (props: Omit<ToastProps, "id">) => {
      const id = Math.random().toString(36).substring(2, 11);
      const newToast: ToastState = { ...props, id, createdAt: Date.now() };
      setToasts((prev) => [...prev, newToast]);

      const duration = props.duration ?? 3500;
      if (duration > 0) {
        const timer = setTimeout(() => {
          startExitAnimation(id);
        }, duration);
        timersRef.current[id] = timer;
      }

      return id;
    },
    [startExitAnimation]
  );

  const dismiss = React.useCallback(
    (id?: string) => {
      if (id) {
        clearTimer(id);
        startExitAnimation(id);
      } else {
        const lastToast = toasts[toasts.length - 1];
        if (lastToast) {
          clearTimer(lastToast.id);
          startExitAnimation(lastToast.id);
        }
      }
    },
    [toasts, clearTimer, startExitAnimation]
  );

  React.useEffect(() => {
    const timers = timersRef.current;
    return () => {
      for (const id in timers) {
        clearTimeout(timers[id]);
      }
    };
  }, []);

  return (
    <ToastContext.Provider value={{ toast, dismiss, toastCount: toasts.length }}>
      {children}
      {toasts.length > 0 && (
        <div className="fixed z-50 flex flex-col items-center gap-1.5 pointer-events-none bottom-3 left-1/2 -translate-x-1/2">
          {toasts.map((t) => (
            <ToastItem key={t.id} {...t} onClose={() => dismiss(t.id)} />
          ))}
        </div>
      )}
    </ToastContext.Provider>
  );
};

const variantConfig = {
  default: {
    icon: Info,
    containerClass: "bg-surface-1 border-border shadow-elevated",
    iconClass: "text-muted-foreground",
    titleClass: "text-foreground",
    descClass: "text-muted-foreground",
  },
  destructive: {
    icon: AlertCircle,
    containerClass: "bg-destructive/10 border-destructive/30 shadow-elevated",
    iconClass: "text-destructive",
    titleClass: "text-destructive",
    descClass: "text-destructive/80",
  },
  success: {
    icon: CheckCircle2,
    containerClass: "bg-success/10 border-success/30 shadow-elevated",
    iconClass: "text-success",
    titleClass: "text-success",
    descClass: "text-success/80",
  },
};

const ToastItem: React.FC<ToastState & { onClose?: () => void }> = ({
  title,
  description,
  action,
  variant = "default",
  isExiting,
  onClose,
}) => {
  const config = variantConfig[variant];
  const Icon = config.icon;

  return (
    <div
      className={cn(
        "pointer-events-auto relative flex items-start gap-2.5 w-[360px] max-w-[calc(100vw-1.5rem)]",
        "px-3 py-2.5 pr-8 overflow-hidden",
        "rounded border backdrop-blur-xl",
        "transition-all duration-200 ease-out",
        isExiting
          ? "opacity-0 translate-x-2 scale-[0.98]"
          : "opacity-100 translate-x-0 scale-100 animate-in slide-in-from-right-4 fade-in-0 duration-300",
        config.containerClass
      )}
    >
      <Icon className={cn("size-4 shrink-0 mt-0.5", config.iconClass)} />
      <div className="flex-1 min-w-0">
        {title && (
          <div className={cn("text-sm font-medium leading-tight", config.titleClass)}>
            {title}
          </div>
        )}
        {description && (
          <div className={cn("text-[13px] leading-snug mt-0.5", config.descClass)}>
            {description}
          </div>
        )}
      </div>
      {action && <div className="shrink-0 self-center">{action}</div>}
      {onClose && (
        <button
          onClick={onClose}
          className="absolute right-1.5 top-1.5 p-1 rounded opacity-50 hover:opacity-100 hover:bg-white/10 transition-all duration-150"
        >
          <X className="size-3.5" />
        </button>
      )}
    </div>
  );
};
