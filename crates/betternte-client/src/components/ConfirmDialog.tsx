import { AlertTriangle, X } from "lucide-react";

export function ConfirmDialog({
  open,
  title,
  message,
  detail,
  confirmLabel = "确定",
  cancelLabel = "取消",
  destructive = false,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title: string;
  message: string;
  detail?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50">
      <div className="bg-card border border-border rounded-lg shadow-lg w-full max-w-md mx-4">
        <div className="flex items-center gap-3 px-5 py-4 border-b border-border-subtle">
          <AlertTriangle
            className={`w-5 h-5 shrink-0 ${destructive ? "text-destructive" : "text-warning"}`}
          />
          <h2 className="text-base font-semibold text-foreground flex-1">{title}</h2>
          <button
            type="button"
            onClick={onCancel}
            className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="px-5 py-4 space-y-2">
          <p className="text-sm text-foreground">{message}</p>
          {detail ? (
            <p className="text-xs text-foreground-tertiary font-mono break-all">{detail}</p>
          ) : null}
        </div>
        <div className="flex justify-end gap-2 px-5 py-3 border-t border-border-subtle">
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-1.5 rounded-md bg-surface border border-border text-sm text-foreground hover:bg-surface-hover"
          >
            {cancelLabel}
          </button>
          <button
            type="button"
            onClick={onConfirm}
            className={`px-4 py-1.5 rounded-md text-sm font-medium ${
              destructive
                ? "bg-destructive text-destructive-foreground hover:bg-destructive/90"
                : "bg-primary text-primary-foreground hover:bg-primary-hover"
            }`}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
