import type { ReactNode } from "react";

import { HelpHint } from "@/components/ui/HelpHint";

export interface SettingRowProps {
  label: string;
  description?: string;
  /** Long help text: wider tooltip with scroll */
  helpWide?: boolean;
  children: ReactNode;
}

export function SettingRow({ label, description, helpWide, children }: SettingRowProps) {
  return (
    <div className="flex items-center justify-between py-1 gap-2">
      <div className="min-w-0 flex items-center gap-1.5">
        <div className="text-sm text-foreground">{label}</div>
        {description && <HelpHint text={description} wide={helpWide} />}
      </div>
      <div className="shrink-0 ml-2">{children}</div>
    </div>
  );
}
