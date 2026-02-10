import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/utils/cn";

const buttonVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 whitespace-nowrap",
    "rounded text-sm font-medium cursor-pointer select-none",
    "transition-all duration-200 ease-out",
    "outline-none focus-visible:ring-2 focus-visible:ring-ring/30 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
    "disabled:pointer-events-none disabled:opacity-50 disabled:cursor-not-allowed",
    "[&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 [&_svg]:shrink-0 shrink-0",
  ].join(" "),
  {
    variants: {
      variant: {
        default: [
          "relative text-primary-foreground font-semibold tracking-[0.005em]",
          "bg-primary border border-primary/60 shadow-sm",
          "hover:bg-primary/95 hover:shadow",
          "active:bg-primary/85 active:scale-[0.985]",
        ].join(" "),
        success: [
          "relative text-success-foreground font-semibold",
          "bg-success border border-success/70 shadow-sm",
          "hover:bg-success/90",
          "active:bg-success/80 active:scale-[0.98]",
        ].join(" "),
        destructive: [
          "relative text-destructive-foreground font-semibold",
          "bg-destructive border border-destructive/70 shadow-sm",
          "hover:bg-destructive/90",
          "active:bg-destructive/80 active:scale-[0.98]",
        ].join(" "),
        outline: [
          "relative font-medium text-foreground",
          "bg-surface-raised/90 border border-border-hover",
          "shadow-sm hover:bg-surface-raised",
          "active:scale-[0.985]",
        ].join(" "),
        secondary: [
          "relative font-medium",
          "text-foreground/90 bg-white/8 border border-white/5",
          "hover:bg-white/12",
          "active:scale-[0.98]",
        ].join(" "),
        ghost: [
          "font-medium text-foreground/90",
          "hover:bg-white/8",
          "active:scale-[0.98]",
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
