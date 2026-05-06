import { ArrowUpCircle,Check, Download, Search, Star } from "lucide-react";
import { useState } from "react";

import { mockStoreScripts } from "@/lib/mock";
import type { StoreScript } from "@/lib/types";
import { cn } from "@/lib/utils";

const categories = [
  { id: "all", label: "全部" },
  { id: "combat", label: "战斗" },
  { id: "daily", label: "日常" },
  { id: "explore", label: "探索" },
  { id: "tool", label: "工具" },
  { id: "trigger", label: "触发器" },
];

function ScriptCard({ script }: { script: StoreScript }) {
  return (
    <div className="rounded-lg border border-border-subtle bg-card p-4 hover:bg-card-hover hover:border-border transition-colors flex flex-col">
      <div className="flex items-start justify-between mb-2">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold text-foreground truncate">
            {script.display_name}
          </h3>
          <div className="text-xs text-foreground-tertiary mt-0.5">
            {script.author} &middot; v{script.version}
          </div>
        </div>
        <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-primary/15 text-primary shrink-0 ml-2">
          {categories.find((c) => c.id === script.category)?.label ?? script.category}
        </span>
      </div>

      <p className="text-xs text-foreground-secondary line-clamp-2 mb-3 flex-1">
        {script.description}
      </p>

      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3 text-xs text-foreground-tertiary">
          <span className="flex items-center gap-1">
            <Star className="w-3 h-3 text-warning fill-warning" />
            {script.rating}
          </span>
          <span className="flex items-center gap-1">
            <Download className="w-3 h-3" />
            {script.downloads >= 1000
              ? `${(script.downloads / 1000).toFixed(1)}k`
              : script.downloads}
          </span>
        </div>

        {script.installed ? (
          script.update_available ? (
            <button className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-primary/15 text-primary text-xs font-medium hover:bg-primary/25">
              <ArrowUpCircle className="w-3.5 h-3.5" />
              更新
            </button>
          ) : (
            <span className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-success/15 text-success text-xs font-medium">
              <Check className="w-3.5 h-3.5" />
              已安装
            </span>
          )
        ) : (
          <button className="px-3 py-1.5 rounded-md bg-primary text-primary-foreground text-xs font-medium hover:bg-primary-hover">
            安装
          </button>
        )}
      </div>
    </div>
  );
}

export function ScriptStore() {
  const [search, setSearch] = useState("");
  const [activeCategory, setActiveCategory] = useState("all");

  const filtered = mockStoreScripts.filter((s) => {
    if (activeCategory !== "all" && s.category !== activeCategory) return false;
    if (
      search &&
      !s.display_name.includes(search) &&
      !s.name.includes(search) &&
      !s.description.includes(search)
    )
      return false;
    return true;
  });

  return (
    <div className="p-6 max-w-5xl">
      <div className="flex items-center justify-between mb-5">
        <h1 className="text-lg font-semibold text-foreground">脚本商店</h1>
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-foreground-tertiary" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索脚本..."
            className="pl-9 pr-3 py-2 rounded-md bg-surface border border-border text-sm text-foreground placeholder:text-foreground-tertiary outline-none focus:border-primary w-56"
          />
        </div>
      </div>

      <div className="flex gap-1 mb-5 bg-surface/50 rounded-lg p-1 border border-border-subtle overflow-x-auto">
        {categories.map((cat) => (
          <button
            key={cat.id}
            onClick={() => setActiveCategory(cat.id)}
            className={cn(
              "px-3 py-1.5 rounded-md text-sm font-medium whitespace-nowrap transition-colors",
              activeCategory === cat.id
                ? "bg-card text-foreground shadow-sm"
                : "text-foreground-tertiary hover:text-foreground-secondary"
            )}
          >
            {cat.label}
          </button>
        ))}
      </div>

      {filtered.length === 0 ? (
        <div className="flex items-center justify-center h-64 text-foreground-tertiary">
          没有找到匹配的脚本
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {filtered.map((script) => (
            <ScriptCard key={script.name} script={script} />
          ))}
        </div>
      )}
    </div>
  );
}
