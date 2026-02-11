import * as React from "react";
import { cn } from "@/utils/cn";

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        "flex h-9 w-full min-w-0 rounded-lg border border-border bg-surface-1 px-3 py-2 text-sm text-foreground shadow-none transition-all duration-200 outline-none",
        "placeholder:text-muted-foreground/60",
        "selection:bg-primary/20 selection:text-foreground",
        "file:text-foreground file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:font-medium",
        "disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 disabled:bg-muted",
        "hover:border-border-hover",
        "focus:border-border-active focus:ring-1 focus:ring-primary/20",
        "aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
        className
      )}
      {...props}
    />
  );
}

export { Input };
