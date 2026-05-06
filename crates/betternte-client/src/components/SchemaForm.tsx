import { HelpHint } from "@/components/ui/HelpHint";
import { cn } from "@/lib/utils";

// ============================================================================
// JSON Schema dynamic form renderer (shared)
// ============================================================================

export function SchemaField({
  name,
  schema,
  value,
  onChange,
  disabled,
}: {
  name: string;
  schema: Record<string, unknown>;
  value: unknown;
  onChange: (name: string, value: unknown) => void;
  disabled?: boolean;
}) {
  const type = (schema.type as string) ?? "string";
  const title = (schema.title as string) ?? name;
  const description = schema.description as string | undefined;
  const defaultValue = schema.default;

  const currentValue = value ?? defaultValue;

  return (
    <div className={cn("flex items-center justify-between py-1 gap-2", disabled && "opacity-50")}>
      <div className="min-w-0 flex items-center gap-1.5">
        <div className="text-sm text-foreground">{title}</div>
        {description && <HelpHint text={description} />}
      </div>
      <div className="shrink-0 ml-4">
        {type === "boolean" ? (
          <button
            type="button"
            onClick={() => {
              if (!disabled) onChange(name, !currentValue);
            }}
            disabled={disabled}
            className={cn(
              "w-10 h-5 rounded-full relative transition-colors",
              currentValue ? "bg-primary" : "bg-foreground-tertiary/30",
              disabled && "cursor-not-allowed"
            )}
          >
            <span
              className={cn(
                "absolute top-0.5 w-4 h-4 rounded-full bg-white shadow-sm transition-all",
                currentValue ? "left-[22px]" : "left-0.5"
              )}
            />
          </button>
        ) : type === "number" || type === "integer" ? (
          <input
            type="number"
            value={currentValue as number ?? 0}
            min={schema.minimum as number}
            max={schema.maximum as number}
            disabled={disabled}
            onChange={(e) => onChange(name, Number(e.target.value))}
            className={cn(
              "bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground outline-none focus:border-primary w-20 text-center font-mono",
              disabled && "cursor-not-allowed"
            )}
          />
        ) : type === "string" && schema.enum ? (
          <select
            value={currentValue as string ?? ""}
            disabled={disabled}
            onChange={(e) => onChange(name, e.target.value)}
            className={cn(
              "bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground outline-none focus:border-primary",
              disabled && "cursor-not-allowed"
            )}
          >
            {(schema.enum as string[]).map((opt) => (
              <option key={opt} value={opt}>
                {opt}
              </option>
            ))}
          </select>
        ) : (
          <input
            type="text"
            value={currentValue as string ?? ""}
            disabled={disabled}
            onChange={(e) => onChange(name, e.target.value)}
            className={cn(
              "bg-surface border border-border rounded-md px-3 py-1.5 text-sm text-foreground placeholder:text-foreground-tertiary outline-none focus:border-primary w-48",
              disabled && "cursor-not-allowed"
            )}
          />
        )}
      </div>
    </div>
  );
}

export function SchemaForm({
  schema,
  values,
  onChange,
  disabled,
  emptyMessage,
}: {
  schema: Record<string, unknown>;
  values: Record<string, unknown>;
  onChange: (values: Record<string, unknown>) => void;
  disabled?: boolean;
  emptyMessage?: string;
}) {
  const properties = (schema.properties as Record<string, Record<string, unknown>>) ?? {};

  if (Object.keys(properties).length === 0) {
    return (
      <div className="text-xs text-foreground-tertiary py-2">
        {emptyMessage ?? "没有可配置的选项"}
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {Object.entries(properties).map(([key, propSchema]) => (
        <SchemaField
          key={key}
          name={key}
          schema={propSchema}
          value={values[key]}
          disabled={disabled}
          onChange={(name, value) => {
            onChange({ ...values, [name]: value });
          }}
        />
      ))}
    </div>
  );
}
