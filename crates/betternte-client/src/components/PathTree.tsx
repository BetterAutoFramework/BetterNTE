import { ChevronDown, ChevronRight, Folder, FolderOpen } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { cn } from "@/lib/utils";

export interface PathTreeItem<T> {
  id: string;
  label: string;
  path?: string;
  data: T;
}

interface TreeNode<T> {
  id: string;
  name: string;
  fullPath: string;
  children: TreeNode<T>[];
  items: PathTreeItem<T>[];
}

interface RenderLeafArgs<T> {
  item: PathTreeItem<T>;
  isSelected: boolean;
  onSelect: () => void;
}

interface PathTreeProps<T> {
  items: PathTreeItem<T>[];
  selectedId?: string | null;
  onSelect: (item: PathTreeItem<T>) => void;
  emptyText: string;
  renderLeaf: (args: RenderLeafArgs<T>) => React.ReactNode;
  className?: string;
}

function normalizePath(path?: string): string {
  if (!path) return "";
  return path.replaceAll("\\", "/").replace(/^\/+|\/+$/g, "");
}

function buildTree<T>(items: PathTreeItem<T>[]): TreeNode<T> {
  const root: TreeNode<T> = {
    id: "__root__",
    name: "",
    fullPath: "",
    children: [],
    items: [],
  };

  const folderMap = new Map<string, TreeNode<T>>();
  folderMap.set("", root);

  const sortedItems = [...items].sort((a, b) => {
    const aPath = normalizePath(a.path);
    const bPath = normalizePath(b.path);
    if (aPath !== bPath) return aPath.localeCompare(bPath, "zh-CN");
    return a.label.localeCompare(b.label, "zh-CN");
  });

  for (const item of sortedItems) {
    const normalized = normalizePath(item.path);
    const segments = normalized ? normalized.split("/") : [];
    let current = root;
    let currentPath = "";

    for (const segment of segments) {
      currentPath = currentPath ? `${currentPath}/${segment}` : segment;
      let node = folderMap.get(currentPath);
      if (!node) {
        node = {
          id: `folder:${currentPath}`,
          name: segment,
          fullPath: currentPath,
          children: [],
          items: [],
        };
        folderMap.set(currentPath, node);
        current.children.push(node);
      }
      current = node;
    }
    current.items.push(item);
  }

  const sortNodes = (node: TreeNode<T>) => {
    node.children.sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));
    node.items.sort((a, b) => a.label.localeCompare(b.label, "zh-CN"));
    node.children.forEach(sortNodes);
  };
  sortNodes(root);

  return root;
}

function countItems<T>(node: TreeNode<T>): number {
  let total = node.items.length;
  for (const child of node.children) {
    total += countItems(child);
  }
  return total;
}

function FolderNode<T>({
  node,
  selectedId,
  expanded,
  onToggleExpand,
  onSelect,
  renderLeaf,
  depth,
}: {
  node: TreeNode<T>;
  selectedId?: string | null;
  expanded: Set<string>;
  onToggleExpand: (folderPath: string) => void;
  onSelect: (item: PathTreeItem<T>) => void;
  renderLeaf: (args: RenderLeafArgs<T>) => React.ReactNode;
  depth: number;
}) {
  const isOpen = expanded.has(node.fullPath);
  const itemCount = countItems(node);
  const paddingLeft = 8 + depth * 12;

  return (
    <div>
      <button
        onClick={() => onToggleExpand(node.fullPath)}
        className="w-full flex items-center gap-1.5 py-1.5 pr-2 rounded-md hover:bg-surface-hover text-xs text-foreground-tertiary hover:text-foreground transition-colors"
        style={{ paddingLeft }}
      >
        {isOpen ? (
          <ChevronDown className="w-3.5 h-3.5 shrink-0" />
        ) : (
          <ChevronRight className="w-3.5 h-3.5 shrink-0" />
        )}
        {isOpen ? (
          <FolderOpen className="w-3.5 h-3.5 shrink-0" />
        ) : (
          <Folder className="w-3.5 h-3.5 shrink-0" />
        )}
        <span className="truncate flex-1 text-left">{node.name}</span>
        <span className="text-[11px] text-foreground-tertiary/70">{itemCount}</span>
      </button>

      {isOpen && (
        <div>
          {node.children.map((child) => (
            <FolderNode
              key={child.id}
              node={child}
              selectedId={selectedId}
              expanded={expanded}
              onToggleExpand={onToggleExpand}
              onSelect={onSelect}
              renderLeaf={renderLeaf}
              depth={depth + 1}
            />
          ))}
          {node.items.map((item) => {
            const isSelected = selectedId === item.id;
            const onSelectItem = () => onSelect(item);
            return (
              <div key={item.id} style={{ paddingLeft: 8 + (depth + 1) * 12 }}>
                {renderLeaf({ item, isSelected, onSelect: onSelectItem })}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function PathTree<T>({
  items,
  selectedId,
  onSelect,
  emptyText,
  renderLeaf,
  className,
}: PathTreeProps<T>) {
  const tree = useMemo(() => buildTree(items), [items]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const selectedAncestors = useMemo(() => {
    if (!selectedId) return [] as string[];
    const selectedItem = items.find((item) => item.id === selectedId);
    if (!selectedItem) return [] as string[];
    const normalized = normalizePath(selectedItem.path);
    if (!normalized) return [] as string[];
    const segments = normalized.split("/");
    const ancestors: string[] = [];
    let currentPath = "";
    for (const segment of segments) {
      currentPath = currentPath ? `${currentPath}/${segment}` : segment;
      ancestors.push(currentPath);
    }
    return ancestors;
  }, [items, selectedId]);

  useEffect(() => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.size === 0) {
        for (const child of tree.children) {
          next.add(child.fullPath);
        }
      }
      for (const folderPath of selectedAncestors) {
        next.add(folderPath);
      }
      return next;
    });
  }, [tree, selectedAncestors]);

  const toggleExpand = (folderPath: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(folderPath)) {
        next.delete(folderPath);
      } else {
        next.add(folderPath);
      }
      return next;
    });
  };

  if (items.length === 0) {
    return (
      <div className={cn("text-xs text-foreground-tertiary py-8 text-center", className)}>
        {emptyText}
      </div>
    );
  }

  return (
    <div className={cn("space-y-0.5", className)}>
      {tree.children.map((child) => (
        <FolderNode
          key={child.id}
          node={child}
          selectedId={selectedId}
          expanded={expanded}
          onToggleExpand={toggleExpand}
          onSelect={onSelect}
          renderLeaf={renderLeaf}
          depth={0}
        />
      ))}
      {tree.items.map((item) => {
        const isSelected = selectedId === item.id;
        const onSelectItem = () => onSelect(item);
        return <div key={item.id}>{renderLeaf({ item, isSelected, onSelect: onSelectItem })}</div>;
      })}
    </div>
  );
}
