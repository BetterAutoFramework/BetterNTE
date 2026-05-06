import { ChevronDown } from "lucide-react";
import { type ReactNode, useState } from "react";

import { HelpHint } from "@/components/ui/HelpHint";
import { cn } from "@/lib/utils";

export interface CardExpanderProps {
  icon: ReactNode;
  title: string;
  description?: string;
  /** Long section intro shown in tooltip (scroll/wide) */
  descriptionWide?: boolean;
  defaultOpen?: boolean;
  headerRight?: ReactNode;
  children: ReactNode;
}

export function CardExpander({
  icon,
  title,
  description,
  descriptionWide,
  defaultOpen = false,
  headerRight,
  children,
}: CardExpanderProps) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <div className="rounded-lg border border-border-subtle bg-card overflow-hidden">
      <div
        onClick={() => setOpen(!open)}
        className="flex items-center justify-between p-4 cursor-pointer hover:bg-card-hover transition-colors"
      >
        <div className="flex items-center gap-3 min-w-0">
          <div className="w-8 h-8 rounded-md bg-primary/10 flex items-center justify-center text-primary shrink-0">
            {icon}
          </div>
          <div className="min-w-0 flex items-center gap-1.5">
            <div className="text-sm font-medium text-foreground">{title}</div>
            {description && (
              <HelpHint text={description} wide={descriptionWide} className="mt-0.5" />
            )}
          </div>
        </div>
        <div className="flex items-center gap-3">
          {headerRight}
          <ChevronDown
            className={cn(
              "w-4 h-4 text-foreground-tertiary transition-transform",
              open && "rotate-180"
            )}
          />
        </div>
      </div>
      {open && (
        <div className="border-t border-border-subtle">
          <div className="p-4 space-y-4">{children}</div>
        </div>
      )}
    </div>
  );
}
