import { CircleHelp } from "lucide-react";
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { cn } from "@/lib/utils";

export interface HelpHintProps {
  text: string;
  className?: string;
  /** Wider tooltips for long explanations */
  wide?: boolean;
}

/**
 * Question-mark icon next to a label; full help text is shown in a fixed-position
 * tooltip on hover/focus (avoids overflow clipping in nested cards).
 */
export function HelpHint({ text, className, wide }: HelpHintProps) {
  const btnRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [coords, setCoords] = useState({ top: 0, left: 0 });

  const updatePosition = useCallback(() => {
    const el = btnRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const maxW = wide ? 360 : 280;
    let left = r.left;
    const top = r.bottom + 6;
    const padding = 8;
    const vw = window.innerWidth;
    if (left + maxW > vw - padding) {
      left = Math.max(padding, vw - maxW - padding);
    }
    setCoords({ top, left });
  }, [wide]);

  useLayoutEffect(() => {
    if (!open) return;
    updatePosition();
    const onScroll = () => updatePosition();
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    return () => {
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
    };
  }, [open, updatePosition]);

  const tooltip =
    open &&
    createPortal(
      <div
        role="tooltip"
        style={{
          position: "fixed",
          top: coords.top,
          left: coords.left,
          zIndex: 99999,
          maxWidth: wide ? "min(22rem, calc(100vw - 1rem))" : "min(17.5rem, calc(100vw - 1rem))",
        }}
        className={cn(
          "rounded-md border border-border bg-card px-2.5 py-2 text-xs leading-relaxed text-foreground shadow-lg",
          wide && "max-h-52 overflow-y-auto"
        )}
      >
        {text}
      </div>,
      document.body
    );

  return (
    <>
      <button
        ref={btnRef}
        type="button"
        title={text}
        aria-label="说明"
        className={cn(
          "inline-flex shrink-0 rounded p-0.5 text-foreground-tertiary hover:text-foreground outline-none focus-visible:ring-2 focus-visible:ring-primary/40",
          className
        )}
        onClick={(e) => e.stopPropagation()}
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
      >
        <CircleHelp className="w-3.5 h-3.5" strokeWidth={2} />
      </button>
      {tooltip}
    </>
  );
}
