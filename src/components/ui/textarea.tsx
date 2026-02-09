import * as React from "react";
import { cn } from "@/utils/cn";

const Textarea = React.forwardRef<HTMLTextAreaElement, React.TextareaHTMLAttributes<HTMLTextAreaElement>>(
  ({ className, ...props }, ref) => {
    return (
      <textarea
        className={cn(
          "flex min-h-[80px] w-full rounded border border-border-subtle/50 bg-surface-1 px-3.5 py-3 text-sm text-foreground shadow-sm transition-all duration-200 outline-none resize-y cursor-text",
          "placeholder:text-muted-foreground/40",
          "hover:border-border-hover",
          "focus:border-border-active focus:ring-2 focus:ring-ring/10",
          "disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50",
          className
        )}
        ref={ref}
        {...props}
      />
    );
  }
);
Textarea.displayName = "Textarea";

export { Textarea };
