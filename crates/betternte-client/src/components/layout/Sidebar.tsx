import {
  Keyboard,
  ListTodo,
  PanelLeftClose,
  PanelLeftOpen,
  Play,
  Settings,
  Turtle,
  Zap,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { NavLink } from "react-router-dom";

import { SIDEBAR_PANEL_THRESHOLD_PX } from "@/lib/constants/layout";
import { cn } from "@/lib/utils";

interface NavItem {
  to: string;
  icon: React.ReactNode;
  label: string;
  badge?: number;
}

const navItems: NavItem[] = [
  { to: "/", icon: <Play className="w-5 h-5" />, label: "启动" },
  { to: "/triggers", icon: <Zap className="w-5 h-5" />, label: "触发器" },
  { to: "/scripts", icon: <ListTodo className="w-5 h-5" />, label: "脚本" },
  { to: "/one-dragon", icon: <Turtle className="w-5 h-5" />, label: "任务组" },
];

const COLLAPSED_KEY = "betternte-sidebar-collapsed";
const DEV_MODE_KEY = "betternte-developer-mode";

function NavItemLink({ item, collapsed }: { item: NavItem; collapsed: boolean }) {
  return (
    <NavLink
      to={item.to}
      end={item.to === "/"}
      title={collapsed ? item.label : undefined}
      className={({ isActive }) =>
        cn(
          "flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-colors",
          collapsed && "justify-center px-0",
          isActive
            ? "bg-primary/15 text-primary"
            : "text-foreground-secondary hover:text-foreground hover:bg-surface-hover"
        )
      }
    >
      {item.icon}
      {!collapsed && <span>{item.label}</span>}
      {!collapsed && item.badge !== undefined && item.badge > 0 && (
        <span className="ml-auto text-xs bg-primary text-primary-foreground rounded-full px-1.5 py-0.5 min-w-[20px] text-center">
          {item.badge}
        </span>
      )}
    </NavLink>
  );
}


export function Sidebar() {
  const [collapsed, setCollapsed] = useState(() => {
    return localStorage.getItem(COLLAPSED_KEY) === "true";
  });
  const [devMode, setDevMode] = useState(() => {
    return localStorage.getItem(DEV_MODE_KEY) === "true";
  });
  const manualRef = useRef(false);

  useEffect(() => {
    localStorage.setItem(COLLAPSED_KEY, String(collapsed));
  }, [collapsed]);

  // Auto-collapse on narrow window, auto-expand on wide window
  useEffect(() => {
    const onResize = () => {
      const w = window.innerWidth;
      if (w < SIDEBAR_PANEL_THRESHOLD_PX) {
        setCollapsed(true);
      } else if (!manualRef.current) {
        setCollapsed(false);
      }
    };
    // Run once on mount
    onResize();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  // Track manual toggle
  const toggle = () => {
    manualRef.current = true;
    setCollapsed((c) => !c);
  };

  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      setDevMode(Boolean(detail));
    };
    window.addEventListener("developer-mode-changed", handler);
    return () => window.removeEventListener("developer-mode-changed", handler);
  }, []);

  return (
    <aside
      className={cn(
        "flex flex-col bg-surface/50 border-r border-border-subtle no-select transition-all duration-200",
        collapsed ? "w-16" : "w-56"
      )}
    >
      <div className="flex-1 overflow-y-auto py-3 px-2 space-y-0.5">
        {navItems.map((item) => (
          <NavItemLink key={item.to} item={item} collapsed={collapsed} />
        ))}
        {devMode && (
          <NavItemLink
            item={{ to: "/input-test", icon: <Keyboard className="w-5 h-5" />, label: "输入测试" }}
            collapsed={collapsed}
          />
        )}
      </div>

      <div className="px-2 py-2 border-t border-border-subtle space-y-0.5">
        <NavLink
          to="/settings"
          title={collapsed ? "设置" : undefined}
          className={({ isActive }) =>
            cn(
              "flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-colors",
              collapsed && "justify-center px-0",
              isActive
                ? "bg-primary/15 text-primary"
                : "text-foreground-secondary hover:text-foreground hover:bg-surface-hover"
            )
          }
        >
          <Settings className="w-5 h-5" />
          {!collapsed && <span>设置</span>}
        </NavLink>

        <button
          onClick={toggle}
          title={collapsed ? "展开侧边栏" : "收起侧边栏"}
          className={cn(
            "flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-colors w-full",
            collapsed && "justify-center px-0",
            "text-foreground-tertiary hover:text-foreground hover:bg-surface-hover"
          )}
        >
          {collapsed ? (
            <PanelLeftOpen className="w-5 h-5" />
          ) : (
            <>
              <PanelLeftClose className="w-5 h-5" />
              <span>收起</span>
            </>
          )}
        </button>
      </div>
    </aside>
  );
}
