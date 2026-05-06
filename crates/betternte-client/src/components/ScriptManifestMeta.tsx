import { Cpu, Layers, Shield, Tag, User } from "lucide-react";
import { useMemo } from "react";

import { HelpHint } from "@/components/ui/HelpHint";
import type { ScriptInfo, TaskGroupNode } from "@/lib/types";
import { cn } from "@/lib/utils";

const PERMISSION_LABELS: Record<string, string> = {
  screenshot: "截图",
  click: "点击",
  input: "输入控制",
  keyboard: "键盘输入",
  mouse: "鼠标输入",
  ocr: "OCR 识别",
  template_match: "模板匹配",
  color_detect: "颜色检测",
  window: "窗口访问",
  storage: "本地存储",
  network: "网络请求",
  file: "文件访问",
  notify: "通知",
  call_script: "调用脚本",
  call_library: "调用库",
};

function permissionLabel(permission: string): string {
  return PERMISSION_LABELS[permission] ?? `未定义权限 (${permission})`;
}

// ============================================================================
// Single script / trigger manifest metadata (author, permissions, etc.)
// ============================================================================

export function ScriptManifestMetaSection({
  script,
  title = "清单信息",
  className,
  showDescription = true,
}: {
  script: ScriptInfo;
  title?: string;
  className?: string;
  showDescription?: boolean;
}) {
  const perms = script.permissions ?? [];
  const dash = "—";

  return (
    <div className={cn("space-y-3", className)}>
      <div className="text-sm font-medium text-foreground">
        {title}
      </div>
      <div className="rounded-lg border border-border-subtle bg-card/50 p-4 text-sm">
        <dl className="space-y-2.5">
          <div className="flex gap-2">
            <dt className="flex items-center gap-1.5 text-foreground-tertiary shrink-0 w-24">
              <Layers className="w-3.5 h-3.5" />
              版本
            </dt>
            <dd className="font-mono text-foreground">v{script.version}</dd>
          </div>
          <div className="flex gap-2">
            <dt className="flex items-center gap-1.5 text-foreground-tertiary shrink-0 w-24">
              <User className="w-3.5 h-3.5" />
              作者
            </dt>
            <dd className="text-foreground">{script.author?.trim() || dash}</dd>
          </div>
          {script.min_engine_version ? (
            <div className="flex gap-2">
              <dt className="flex items-center gap-1.5 text-foreground-tertiary shrink-0 w-24">
                <Cpu className="w-3.5 h-3.5" />
                最低引擎
              </dt>
              <dd className="font-mono text-foreground">{script.min_engine_version}</dd>
            </div>
          ) : null}
          {script.tags && script.tags.length > 0 ? (
            <div className="flex gap-2 items-start">
              <dt className="flex items-center gap-1.5 text-foreground-tertiary shrink-0 w-24 pt-0.5">
                <Tag className="w-3.5 h-3.5" />
                标签
              </dt>
              <dd className="flex-1 min-w-0">
                <div className="flex flex-wrap gap-1.5">
                  {script.tags.map((tag) => (
                    <span
                      key={tag}
                      className="px-2 py-0.5 rounded text-xs bg-surface text-foreground-tertiary"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              </dd>
            </div>
          ) : null}
          <div className="flex gap-2 items-start">
            <dt className="flex items-center gap-1.5 text-foreground-tertiary shrink-0 w-24 pt-0.5">
              <Shield className="w-3.5 h-3.5" />
              权限
            </dt>
            <dd className="flex-1 min-w-0">
              {perms.length === 0 ? (
                <span className="text-foreground-tertiary">无</span>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {[...perms].sort((a, b) => a.localeCompare(b)).map((p) => (
                    <span
                      key={p}
                      className="px-2 py-0.5 rounded-md text-xs font-mono bg-surface border border-border-subtle text-foreground-secondary"
                    >
                      {permissionLabel(p)}
                    </span>
                  ))}
                </div>
              )}
            </dd>
          </div>
          {showDescription && script.description?.trim() ? (
            <div className="flex gap-2 items-start pt-2 border-t border-border-subtle/80">
              <dt className="text-foreground-tertiary shrink-0 w-24">描述</dt>
              <dd className="text-foreground-secondary leading-relaxed">{script.description}</dd>
            </div>
          ) : null}
        </dl>
      </div>
    </div>
  );
}

// ============================================================================
// Task group: union of permissions from all referenced script manifests
// ============================================================================

export function TaskGroupPermissionsUnionSection({
  nodes,
  scripts,
  title = "清单信息",
}: {
  nodes: TaskGroupNode[];
  scripts: ScriptInfo[];
  title?: string;
}) {
  const { sorted, missing } = useMemo(() => {
    const byName = new Map(scripts.map((s) => [s.name, s]));
    const set = new Set<string>();
    const missingNames: string[] = [];
    for (const n of nodes) {
      const sc = byName.get(n.script);
      if (!sc) {
        if (!missingNames.includes(n.script)) missingNames.push(n.script);
        continue;
      }
      for (const p of sc.permissions ?? []) {
        set.add(p);
      }
    }
    return {
      sorted: Array.from(set).sort((a, b) => a.localeCompare(b)),
      missing: missingNames,
    };
  }, [nodes, scripts]);

  return (
    <div className="space-y-3">
      <div className="text-sm font-medium text-foreground">{title}</div>
      <div className="rounded-lg border border-border-subtle bg-card/50 p-4 space-y-2 text-sm">
        <div className="flex items-center gap-1.5">
          <div className="text-xs font-medium text-foreground-secondary uppercase tracking-wider">
            权限（节点脚本并集）
          </div>
          <HelpHint text="这里展示任务组里所有脚本权限的汇总（已自动去重）。" />
        </div>
        {missing.length > 0 ? (
          <div className="text-xs text-warning">
            未在本地脚本列表中找到：{missing.join("、")}
          </div>
        ) : null}
        <div className="flex flex-wrap gap-1.5 pt-1">
          {sorted.length === 0 ? (
            <span className="text-foreground-tertiary">无（或脚本均未声明权限）</span>
          ) : (
            sorted.map((p) => (
              <span
                key={p}
                className="px-2 py-0.5 rounded-md text-xs font-mono bg-surface border border-border-subtle text-foreground-secondary"
              >
                {permissionLabel(p)}
              </span>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
