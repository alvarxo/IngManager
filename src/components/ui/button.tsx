import { cva, type VariantProps } from "class-variance-authority";
import type { ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-1.5 rounded-[var(--radius-sm)] px-2.5 h-[32px] text-[12.5px] font-medium transition-colors disabled:opacity-40 disabled:pointer-events-none",
  {
    variants: {
      variant: {
        default: "bg-accent text-white hover:bg-[#2563eb]",
        secondary:
          "bg-elevated text-text border border-border hover:bg-[#243041]",
        ghost: "bg-transparent text-muted hover:bg-elevated hover:text-text",
        danger: "bg-error/15 text-error border border-error/30 hover:bg-error/25",
      },
      size: {
        default: "",
        sm: "h-[28px] px-2 text-[12px]",
        icon: "h-[32px] w-[32px] px-0",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {}

export function Button({ className, variant, size, ...props }: ButtonProps) {
  return (
    <button className={cn(buttonVariants({ variant, size }), className)} {...props} />
  );
}
