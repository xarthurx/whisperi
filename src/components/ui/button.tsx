import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/utils/cn";

const buttonVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 whitespace-nowrap",
    "rounded text-sm font-medium cursor-pointer select-none",
    "transition-all duration-150",
    "outline-none focus-visible:ring-2 focus-visible:ring-ring/30 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
    "disabled:pointer-events-none disabled:opacity-50 disabled:cursor-not-allowed",
    "[&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 [&_svg]:shrink-0 shrink-0",
  ].join(" "),
  {
    variants: {
      variant: {
        default: [
          "bg-transparent text-foreground",
          "hover:bg-surface-1",
        ].join(" "),
        success: [
          "bg-transparent text-success",
          "hover:bg-success/10",
        ].join(" "),
        destructive: [
          "bg-transparent text-destructive",
          "hover:bg-destructive/10",
        ].join(" "),
        outline: [
          "border border-border bg-transparent text-foreground",
          "hover:bg-surface-1",
        ].join(" "),
        secondary: [
          "bg-surface-1 text-foreground",
          "hover:bg-surface-2",
        ].join(" "),
        ghost: [
          "text-muted-foreground",
          "hover:text-foreground hover:bg-surface-1",
        ].join(" "),
        link: [
          "font-medium text-primary",
          "hover:text-primary/80 hover:underline underline-offset-4",
        ].join(" "),
      },
      size: {
        default: "h-10 px-4 py-2",
        sm: "h-8 px-3 text-sm gap-1.5",
        lg: "h-12 px-6 text-sm",
        icon: "size-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
);

function Button({
  className,
  variant,
  size,
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean;
  }) {
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      data-slot="button"
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  );
}

export { Button, buttonVariants };
