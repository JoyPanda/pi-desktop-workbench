import type { ReactNode } from "react";
import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function Shell({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return <div className={`shell ${className}`}>{children}</div>;
}

export function Band({
  children,
  className = "",
  dashed = false,
  id,
}: {
  children: ReactNode;
  className?: string;
  dashed?: boolean;
  id?: string;
}) {
  return (
    <section
      id={id}
      className={`band ${dashed ? "band-dashed" : ""} ${className}`}
    >
      {children}
    </section>
  );
}

/** Orbit's button intents mapped onto shadcn's `buttonVariants`. */
const buttonIntents = {
  primary: "default",
  secondary: "outline",
  nav: "default",
  text: "ghost",
} as const;

export type ButtonIntent = keyof typeof buttonIntents;

/**
 * The link-button styles on their own, so a control that is one visual button
 * split into two segments (a primary action plus its options menu) can style
 * both halves from the same intent.
 */
export function buttonLinkClass(
  variant: ButtonIntent = "primary",
  className = "",
) {
  return cn(
    buttonVariants({
      variant: buttonIntents[variant],
      size: variant === "nav" ? "sm" : "lg",
    }),
    /* Primary: the highest-contrast element on the page. */
    variant === "primary" &&
      "min-h-11 gap-2.5 rounded-[10px] px-5 text-[13px] font-semibold shadow-[0_1px_2px_rgba(0,0,0,0.18)] dark:border-white/20 light:border-black/10",
    variant === "secondary" &&
      "min-h-11 gap-2.5 rounded-[10px] border-edge-default px-5 text-[13px]",
    variant === "nav" &&
      "h-9 min-h-9 rounded-[10px] px-4 text-[13px] font-semibold",
    variant === "text" &&
      "px-0 text-[13px] text-ink-2 hover:bg-transparent hover:text-ink",
    className,
  );
}

export function ButtonLink({
  href,
  children,
  variant = "primary",
  className = "",
  external = false,
}: {
  href: string;
  children: ReactNode;
  variant?: ButtonIntent;
  className?: string;
  external?: boolean;
}) {
  return (
    <a
      href={href}
      className={buttonLinkClass(variant, className)}
      {...(external ? { target: "_blank", rel: "noreferrer" } : {})}
    >
      {children}
    </a>
  );
}

export function SectionLabel({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <p
      className={`flex items-center gap-[9px] font-mono text-[10px] uppercase leading-[1.4] tracking-[0.065em] text-ink-2 ${className}`}
    >
      <i className="pulse-dot size-[5px] shrink-0 rounded-full bg-brand" />
      {children}
    </p>
  );
}

export function Eyebrow({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <p
      className={`font-mono text-[11.5px] uppercase leading-normal tracking-[0.16em] text-ink-3 ${className}`}
    >
      {children}
    </p>
  );
}
