import { useEffect, useState } from "react";
import { X } from "lucide-react";

import { cn } from "@/lib/utils";

export function HotkeyInput({
  value,
  onChange,
  disabled,
  allowClear = true,
}: {
  value: string;
  onChange: (v: string) => void;
  disabled?: boolean;
  /** When true, Esc during recording and the clear button unset the binding (empty string). */
  allowClear?: boolean;
}) {
  const [recording, setRecording] = useState(false);
  const trimmed = value.trim();
  const showClear = allowClear && !disabled && trimmed.length > 0;

  useEffect(() => {
    if (!recording || disabled) return;
    const handler = (e: KeyboardEvent) => {
      e.preventDefault();
      if (allowClear && e.key === "Escape") {
        onChange("");
        setRecording(false);
        return;
      }
      const parts: string[] = [];
      if (e.ctrlKey) parts.push("Ctrl");
      if (e.altKey) parts.push("Alt");
      if (e.shiftKey) parts.push("Shift");
      if (e.metaKey) parts.push("Meta");
      if (!["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
        parts.push(e.key.length === 1 ? e.key.toUpperCase() : e.key);
      }
      if (parts.length > 0) {
        onChange(parts.join("+"));
        setRecording(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [recording, onChange, disabled, allowClear]);

  return (
    <span className="inline-flex items-center gap-1">
      <button
        type="button"
        disabled={disabled}
        onClick={() => !disabled && setRecording(true)}
        className={cn(
          "bg-surface border rounded-md px-3 py-1.5 text-sm font-mono text-foreground min-w-[60px] text-center",
          disabled ? "opacity-50 cursor-not-allowed" : "",
          recording ? "border-primary animate-pulse" : "border-border hover:bg-surface-hover"
        )}
      >
        {recording ? "按下按键… · Esc 清除" : value || "（未设置）"}
      </button>
      {showClear ? (
        <button
          type="button"
          title="清除快捷键"
          aria-label="清除快捷键"
          disabled={disabled}
          onClick={(e) => {
            e.stopPropagation();
            onChange("");
            setRecording(false);
          }}
          className={cn(
            "rounded-md border border-border-subtle p-1 text-foreground-tertiary hover:bg-surface-hover hover:text-foreground",
            disabled ? "opacity-50 cursor-not-allowed pointer-events-none" : ""
          )}
        >
          <X className="h-4 w-4" aria-hidden />
        </button>
      ) : null}
    </span>
  );
}
