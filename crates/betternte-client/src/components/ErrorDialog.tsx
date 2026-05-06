import { AlertCircle,X } from "lucide-react";

import { useEngineStore } from "@/lib/store";

export function ErrorDialog() {
  const errorDialog = useEngineStore((s) => s.errorDialog);
  const closeError = useEngineStore((s) => s.closeError);

  if (!errorDialog) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-card border border-border rounded-lg shadow-lg w-full max-w-lg mx-4">
        {/* Header */}
        <div className="flex items-center gap-3 px-5 py-4 border-b border-border-subtle">
          <AlertCircle className="w-5 h-5 text-destructive shrink-0" />
          <h2 className="text-base font-semibold text-foreground flex-1">
            {errorDialog.title}
          </h2>
          <button
            onClick={closeError}
            className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body */}
        <div className="px-5 py-4 space-y-3">
          <p className="text-sm text-foreground">{errorDialog.message}</p>
          {errorDialog.detail && (
            <pre className="text-xs text-foreground-tertiary bg-surface rounded-md p-3 overflow-auto max-h-48 whitespace-pre-wrap break-all font-mono">
              {errorDialog.detail}
            </pre>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-end px-5 py-3 border-t border-border-subtle">
          <button
            onClick={closeError}
            className="px-4 py-1.5 rounded-md bg-surface border border-border text-sm text-foreground hover:bg-surface-hover"
          >
            确定
          </button>
        </div>
      </div>
    </div>
  );
}
