import { cn } from "@/lib/utils";

export interface ToggleProps {
  checked: boolean;
  onChange: (v: boolean) => void;
  className?: string;
}

export function Toggle({ checked, onChange, className }: ToggleProps) {
  return (
    <button
      type="button"
      onClick={() => onChange(!checked)}
      className={cn(
        "w-10 h-5 rounded-full relative transition-colors shrink-0",
        checked ? "bg-primary" : "bg-foreground-tertiary/30",
        className
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 w-4 h-4 rounded-full bg-white shadow-sm transition-all",
          checked ? "left-[22px]" : "left-0.5"
        )}
      />
    </button>
  );
}
