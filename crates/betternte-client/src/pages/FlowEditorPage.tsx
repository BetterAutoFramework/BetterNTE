import "@xyflow/react/dist/style.css";

import dagre from "@dagrejs/dagre";
import {
  addEdge,
  Background,
  Controls,
  type Edge,
  Handle,
  MiniMap,
  type Node,
  type NodeProps,
  type NodeTypes,
  type OnConnect,
  Panel,
  Position,
  ReactFlow,
  useEdgesState,
  useNodesState,
} from "@xyflow/react";
import {
  Ban,
  ChevronRight,
  Clock,
  Code,
  FolderOpen,
  GitBranch,
  Keyboard,
  Layers,
  LayoutGrid,
  MousePointerClick,
  Move,
  Play,
  Save,
  Settings2,
  Trash2,
  Variable,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { FLOW_EDITOR_STEP_COLORS } from "@/lib/constants/flowEditorPalette";
import { UI_POLL_FAST_MS } from "@/lib/constants/timing";
import { useEngineStore } from "@/lib/store";
import type {
  Condition,
  FlowDefinition,
  FlowStep,
  StepKind,
  StepKindType,
  TaskGroupProgress,
} from "@/lib/types";
import { cn } from "@/lib/utils";

// ============================================================================
// StepKind metadata
// ============================================================================

const STEP_KIND_META: Record<
  StepKindType,
  { label: string; icon: React.ReactNode; color: string }
> = {
  script: {
    label: "脚本",
    icon: <Code className="w-4 h-4" />,
    color: FLOW_EDITOR_STEP_COLORS.script,
  },
  click: {
    label: "点击",
    icon: <MousePointerClick className="w-4 h-4" />,
    color: FLOW_EDITOR_STEP_COLORS.click,
  },
  swipe: {
    label: "滑动",
    icon: <Move className="w-4 h-4" />,
    color: FLOW_EDITOR_STEP_COLORS.swipe,
  },
  key_press: {
    label: "按键",
    icon: <Keyboard className="w-4 h-4" />,
    color: FLOW_EDITOR_STEP_COLORS.key_press,
  },
  wait: {
    label: "等待",
    icon: <Clock className="w-4 h-4" />,
    color: FLOW_EDITOR_STEP_COLORS.wait,
  },
  flow: {
    label: "子流程",
    icon: <GitBranch className="w-4 h-4" />,
    color: FLOW_EDITOR_STEP_COLORS.flow,
  },
  group: {
    label: "任务组",
    icon: <Layers className="w-4 h-4" />,
    color: FLOW_EDITOR_STEP_COLORS.group,
  },
  set_variable: {
    label: "设置变量",
    icon: <Variable className="w-4 h-4" />,
    color: FLOW_EDITOR_STEP_COLORS.set_variable,
  },
  none: {
    label: "空操作",
    icon: <Ban className="w-4 h-4" />,
    color: FLOW_EDITOR_STEP_COLORS.none,
  },
};

// ============================================================================
// Condition summary helper
// ============================================================================

function conditionSummary(cond: Condition): string {
  switch (cond.type) {
    case "always":
      return "总是";
    case "template":
      return `模板: ${cond.template}`;
    case "ocr":
      return `OCR: ${cond.expected}`;
    case "color":
      return `颜色: ${cond.color}`;
    case "variable":
      return `变量 ${cond.key} ${cond.op}`;
    case "hotkey":
      return `热键: ${cond.key}`;
    case "script":
      return `脚本: ${cond.script}`;
    case "and":
      return `AND(${cond.conditions.length})`;
    case "or":
      return `OR(${cond.conditions.length})`;
    case "not":
      return `NOT`;
    default:
      return "未知";
  }
}

// ============================================================================
// Step content helper
// ============================================================================

function stepContent(kind: StepKind): string {
  switch (kind.type) {
    case "script":
      return kind.script;
    case "click":
      return `(${kind.x}, ${kind.y})`;
    case "swipe":
      return `(${kind.x1},${kind.y1}) → (${kind.x2},${kind.y2})`;
    case "key_press":
      return kind.key;
    case "wait":
      return `${kind.ms}ms`;
    case "flow":
      return kind.flow;
    case "group":
      return kind.group;
    case "set_variable":
      return `${kind.key} = ${JSON.stringify(kind.value)}`;
    case "none":
      return "";
    default:
      return "";
  }
}

// ============================================================================
// FlowNode — custom node component
// ============================================================================

interface FlowNodeData {
  stepId: string;
  step: FlowStep;
  isEntry: boolean;
  label: string;
  [key: string]: unknown;
}

function FlowNodeComponent({ data, selected }: NodeProps<Node<FlowNodeData>>) {
  const { stepId, step, isEntry, label } = data;
  const meta = STEP_KIND_META[step.kind.type];
  const content = stepContent(step.kind);
  const hasTransitions = step.transitions.length > 0;

  return (
    <div
      className={cn(
        "relative min-w-[180px] rounded-lg border-2 bg-card shadow-md transition-shadow",
        selected ? "shadow-lg" : "shadow-md",
        isEntry ? "border-success" : "border-border"
      )}
      style={{
        borderColor: selected ? meta.color : isEntry ? "#22c55e" : undefined,
      }}
    >
      {/* Entry indicator */}
      {isEntry && (
        <div className="absolute -top-2.5 left-3 px-1.5 py-0.5 bg-success text-white text-[10px] rounded font-medium">
          入口
        </div>
      )}

      {/* Input handle */}
      {!isEntry && (
        <Handle
          type="target"
          position={Position.Left}
          className="!w-3 !h-3 !bg-foreground-tertiary !border-2 !border-background"
        />
      )}

      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2 rounded-t-md"
        style={{ backgroundColor: `${meta.color}15` }}
      >
        <div
          className="w-6 h-6 rounded flex items-center justify-center text-white"
          style={{ backgroundColor: meta.color }}
        >
          {meta.icon}
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-xs font-semibold text-foreground truncate">
            {label || stepId}
          </div>
          <div className="text-[10px] text-foreground-tertiary">
            {meta.label}
          </div>
        </div>
      </div>

      {/* Body */}
      {content && (
        <div className="px-3 py-1.5 text-[11px] text-foreground-secondary font-mono truncate border-t border-border-subtle">
          {content}
        </div>
      )}

      {/* Transitions summary */}
      {hasTransitions && (
        <div className="px-3 py-1.5 border-t border-border-subtle">
          {step.transitions.map((t, i) => (
            <div
              key={i}
              className="text-[10px] text-foreground-tertiary flex items-center gap-1"
            >
              <span
                className="w-1.5 h-1.5 rounded-full shrink-0"
                style={{
                  backgroundColor: t.interrupt ? "#ef4444" : meta.color,
                }}
              />
              <span className="truncate">
                → {t.target} ({conditionSummary(t.condition)})
              </span>
            </div>
          ))}
        </div>
      )}

      {/* Error handler indicator */}
      {step.on_error && (
        <div className="px-3 py-1 border-t border-destructive/30 text-[10px] text-destructive">
          错误处理 → {step.on_error}
        </div>
      )}

      {/* Output handles — one per transition */}
      {step.transitions.map((_, i) => (
        <Handle
          key={`out-${i}`}
          type="source"
          position={Position.Right}
          id={`output-${i}`}
          className="!w-2.5 !h-2.5 !border-2 !border-background"
          style={{ backgroundColor: meta.color, top: `${50 + i * 12}%` }}
        />
      ))}

      {/* Error output handle */}
      {step.on_error && (
        <Handle
          type="source"
          position={Position.Bottom}
          id="on-error"
          className="!w-2.5 !h-2.5 !bg-destructive !border-2 !border-background"
        />
      )}

      {/* Single output if no transitions */}
      {!hasTransitions && !step.on_error && (
        <Handle
          type="source"
          position={Position.Right}
          className="!w-2.5 !h-2.5 !border-2 !border-background"
          style={{ backgroundColor: meta.color }}
        />
      )}
    </div>
  );
}

const nodeTypes: NodeTypes = {
  "flow-node": FlowNodeComponent,
};

// ============================================================================
// useFlowLayout — dagre auto-layout
// ============================================================================

function useFlowLayout() {
  const layoutNodes = useCallback(
    (
      nodes: Node<FlowNodeData>[],
      edges: Edge[],
      direction: "LR" | "TB" = "LR"
    ): Node<FlowNodeData>[] => {
      if (nodes.length === 0) return nodes;

      const g = new dagre.graphlib.Graph();
      g.setDefaultEdgeLabel(() => ({}));
      g.setGraph({
        rankdir: direction,
        nodesep: 60,
        ranksep: 120,
        marginx: 40,
        marginy: 40,
      });

      for (const node of nodes) {
        g.setNode(node.id, { width: 220, height: 120 });
      }

      for (const edge of edges) {
        g.setEdge(edge.source, edge.target);
      }

      dagre.layout(g);

      return nodes.map((node) => {
        const pos = g.node(node.id);
        return {
          ...node,
          position: {
            x: pos.x - 110, // half width
            y: pos.y - 60, // half height
          },
        };
      });
    },
    []
  );

  return { layoutNodes };
}

// ============================================================================
// useFlowCanvasMapping — Flow model → React Flow nodes/edges
// ============================================================================

function useFlowCanvasMapping(flow: FlowDefinition | null) {
  const { layoutNodes } = useFlowLayout();

  const { nodes, edges } = useMemo(() => {
    if (!flow) return { nodes: [], edges: [] };

    // Build nodes
    const rawNodes: Node<FlowNodeData>[] = Object.entries(flow.steps).map(
      ([stepId, step]) => ({
        id: stepId,
        type: "flow-node" as const,
        position: { x: 0, y: 0 }, // will be set by layout
        data: {
          stepId,
          step,
          isEntry: stepId === flow.entry,
          label: stepId,
        },
      })
    );

    // Build edges
    const rawEdges: Edge[] = [];
    for (const [stepId, step] of Object.entries(flow.steps)) {
      for (const [i, transition] of step.transitions.entries()) {
        rawEdges.push({
          id: `${stepId}__${transition.target}__${i}`,
          source: stepId,
          target: transition.target,
          sourceHandle: `output-${i}`,
          animated: transition.interrupt,
          label: conditionSummary(transition.condition),
          labelStyle: { fontSize: 10, fill: "#9ca3af" },
          labelBgStyle: {
            fill: "#1f2937",
            fillOpacity: 0.8,
            rx: 4,
            ry: 4,
          },
          labelBgPadding: [4, 2] as [number, number],
          style: {
            stroke: transition.interrupt ? "#ef4444" : "#6b7280",
            strokeWidth: 1.5,
          },
        });
      }

      // Error handler edge
      if (step.on_error) {
        rawEdges.push({
          id: `${stepId}__on_error__${step.on_error}`,
          source: stepId,
          target: step.on_error,
          sourceHandle: "on-error",
          style: { stroke: "#ef4444", strokeWidth: 2 },
          label: "错误",
          labelStyle: { fontSize: 10, fill: "#ef4444" },
          labelBgStyle: {
            fill: "#1f2937",
            fillOpacity: 0.8,
            rx: 4,
            ry: 4,
          },
          labelBgPadding: [4, 2] as [number, number],
        });
      }
    }

    // Apply dagre layout
    const layouted = layoutNodes(rawNodes, rawEdges);

    return { nodes: layouted, edges: rawEdges };
  }, [flow, layoutNodes]);

  return { nodes, edges };
}

// ============================================================================
// StepPalette — draggable node type list
// ============================================================================

function StepPalette() {
  const kinds: StepKindType[] = [
    "script",
    "click",
    "swipe",
    "key_press",
    "wait",
    "flow",
    "group",
    "set_variable",
    "none",
  ];

  const onDragStart = (event: React.DragEvent, kind: StepKindType) => {
    event.dataTransfer.setData("application/betternte-step-kind", kind);
    event.dataTransfer.effectAllowed = "move";
  };

  return (
    <div className="space-y-1">
      <div className="text-xs font-medium text-foreground-secondary uppercase tracking-wider px-1 mb-2">
        节点类型
      </div>
      {kinds.map((kind) => {
        const meta = STEP_KIND_META[kind];
        return (
          <div
            key={kind}
            draggable
            onDragStart={(e) => onDragStart(e, kind)}
            className="flex items-center gap-2 px-2 py-1.5 rounded-md cursor-grab hover:bg-surface-hover transition-colors border border-transparent hover:border-border-subtle"
          >
            <div
              className="w-6 h-6 rounded flex items-center justify-center text-white shrink-0"
              style={{ backgroundColor: meta.color }}
            >
              {meta.icon}
            </div>
            <span className="text-sm text-foreground">{meta.label}</span>
          </div>
        );
      })}
    </div>
  );
}

// ============================================================================
// StepPropertyPanel — selected node property editor
// ============================================================================

function StepPropertyPanel({
  node,
  onDelete,
  onClose,
}: {
  node: Node<FlowNodeData> | null;
  onUpdate: (id: string, data: Partial<FlowNodeData>) => void;
  onDelete: (id: string) => void;
  onClose: () => void;
}) {
  if (!node) {
    return (
      <div className="flex flex-col items-center justify-center h-48 text-center">
        <Settings2 className="w-8 h-8 text-foreground-tertiary/30 mb-3" />
        <p className="text-sm text-foreground-tertiary">选择节点查看属性</p>
      </div>
    );
  }

  const { stepId, step, isEntry, label } = node.data;
  const meta = STEP_KIND_META[step.kind.type];

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div
            className="w-6 h-6 rounded flex items-center justify-center text-white"
            style={{ backgroundColor: meta.color }}
          >
            {meta.icon}
          </div>
          <span className="text-sm font-semibold text-foreground">
            {label || stepId}
          </span>
        </div>
        <button
          onClick={onClose}
          className="p-1 rounded hover:bg-surface-hover text-foreground-tertiary"
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      {/* Step ID */}
      <div>
        <label className="text-xs text-foreground-tertiary block mb-1">
          步骤 ID
        </label>
        <input
          type="text"
          value={stepId}
          readOnly
          className="w-full bg-surface border border-border rounded-md px-2 py-1.5 text-sm text-foreground font-mono"
        />
      </div>

      {/* Kind */}
      <div>
        <label className="text-xs text-foreground-tertiary block mb-1">
          类型
        </label>
        <div className="text-sm text-foreground">{meta.label}</div>
      </div>

      {/* Kind-specific fields */}
      {step.kind.type === "script" && (
        <div>
          <label className="text-xs text-foreground-tertiary block mb-1">
            脚本名称
          </label>
          <input
            type="text"
            value={step.kind.script}
            readOnly
            className="w-full bg-surface border border-border rounded-md px-2 py-1.5 text-sm text-foreground font-mono"
          />
        </div>
      )}

      {step.kind.type === "click" && (
        <div className="grid grid-cols-2 gap-2">
          <div>
            <label className="text-xs text-foreground-tertiary block mb-1">
              X
            </label>
            <input
              type="number"
              value={step.kind.x}
              readOnly
              className="w-full bg-surface border border-border rounded-md px-2 py-1.5 text-sm text-foreground font-mono"
            />
          </div>
          <div>
            <label className="text-xs text-foreground-tertiary block mb-1">
              Y
            </label>
            <input
              type="number"
              value={step.kind.y}
              readOnly
              className="w-full bg-surface border border-border rounded-md px-2 py-1.5 text-sm text-foreground font-mono"
            />
          </div>
        </div>
      )}

      {step.kind.type === "wait" && (
        <div>
          <label className="text-xs text-foreground-tertiary block mb-1">
            等待时间 (ms)
          </label>
          <input
            type="number"
            value={step.kind.ms}
            readOnly
            className="w-full bg-surface border border-border rounded-md px-2 py-1.5 text-sm text-foreground font-mono"
          />
        </div>
      )}

      {step.kind.type === "key_press" && (
        <div>
          <label className="text-xs text-foreground-tertiary block mb-1">
            按键
          </label>
          <input
            type="text"
            value={step.kind.key}
            readOnly
            className="w-full bg-surface border border-border rounded-md px-2 py-1.5 text-sm text-foreground font-mono"
          />
        </div>
      )}

      {step.kind.type === "set_variable" && (
        <>
          <div>
            <label className="text-xs text-foreground-tertiary block mb-1">
              变量名
            </label>
            <input
              type="text"
              value={step.kind.key}
              readOnly
              className="w-full bg-surface border border-border rounded-md px-2 py-1.5 text-sm text-foreground font-mono"
            />
          </div>
          <div>
            <label className="text-xs text-foreground-tertiary block mb-1">
              值
            </label>
            <input
              type="text"
              value={JSON.stringify(step.kind.value)}
              readOnly
              className="w-full bg-surface border border-border rounded-md px-2 py-1.5 text-sm text-foreground font-mono"
            />
          </div>
        </>
      )}

      {/* Transitions */}
      <div>
        <label className="text-xs text-foreground-tertiary block mb-2">
          转换 ({step.transitions.length})
        </label>
        {step.transitions.length === 0 ? (
          <div className="text-xs text-foreground-tertiary py-2">
            无转换定义
          </div>
        ) : (
          <div className="space-y-2">
            {step.transitions.map((t, i) => (
              <div
                key={i}
                className="rounded-md border border-border-subtle bg-surface p-2"
              >
                <div className="flex items-center gap-1 text-xs">
                  <ChevronRight className="w-3 h-3 text-foreground-tertiary" />
                  <span className="font-mono text-foreground">{t.target}</span>
                  {t.interrupt && (
                    <span className="ml-auto text-[10px] px-1 py-0.5 bg-destructive/15 text-destructive rounded">
                      中断
                    </span>
                  )}
                </div>
                <div className="text-[10px] text-foreground-tertiary mt-1">
                  {conditionSummary(t.condition)} · 优先级 {t.priority}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Error handler */}
      {step.on_error && (
        <div>
          <label className="text-xs text-foreground-tertiary block mb-1">
            错误处理
          </label>
          <div className="text-sm text-destructive font-mono">
            → {step.on_error}
          </div>
        </div>
      )}

      {/* Entry toggle */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-foreground-secondary">入口节点</span>
        <span
          className={cn(
            "text-xs px-2 py-0.5 rounded-full",
            isEntry
              ? "bg-success/15 text-success"
              : "bg-foreground-tertiary/15 text-foreground-tertiary"
          )}
        >
          {isEntry ? "是" : "否"}
        </span>
      </div>

      {/* Actions */}
      <div className="pt-2 border-t border-border-subtle">
        <button
          onClick={() => onDelete(node.id)}
          className="flex items-center gap-1.5 w-full px-3 py-1.5 rounded-md text-sm text-destructive hover:bg-destructive/10 transition-colors"
        >
          <Trash2 className="w-3.5 h-3.5" />
          删除节点
        </button>
      </div>
    </div>
  );
}

// ============================================================================
// Sample flow for testing
// ============================================================================

const SAMPLE_FLOW: FlowDefinition = {
  id: "demo_flow",
  name: "示例工作流",
  description: "一个演示用的工作流",
  version: "1.0.0",
  entry: "check_hp",
  steps: {
    check_hp: {
      kind: { type: "script", script: "check_hp" },
      input: {},
      output: {},
      transitions: [
        {
          target: "heal",
          condition: {
            type: "variable",
            key: "$variables.hp",
            op: "lt",
            value: 30,
          },
          priority: 100,
          interrupt: false,
        },
        {
          target: "attack",
          condition: { type: "always" },
          priority: 0,
          interrupt: false,
        },
      ],
      max_retries: 0,
    },
    heal: {
      kind: { type: "click", x: 500, y: 300 },
      input: {},
      output: {},
      transitions: [
        {
          target: "wait_heal",
          condition: { type: "always" },
          priority: 0,
          interrupt: false,
        },
      ],
      max_retries: 3,
      on_error: "check_hp",
    },
    wait_heal: {
      kind: { type: "wait", ms: 2000 },
      input: {},
      output: {},
      transitions: [
        {
          target: "check_hp",
          condition: { type: "always" },
          priority: 0,
          interrupt: false,
        },
      ],
      max_retries: 0,
    },
    attack: {
      kind: { type: "script", script: "auto_attack" },
      input: {},
      output: {},
      transitions: [
        {
          target: "check_hp",
          condition: { type: "always" },
          priority: 0,
          interrupt: false,
        },
      ],
      max_retries: 0,
    },
  },
  variables: {
    hp: {
      value_type: "integer",
      default: 100,
      persist: false,
    },
  },
  tags: ["demo"],
};

// ============================================================================
// FlowEditorPage
// ============================================================================

export function FlowEditorPage() {
  const flows = useEngineStore((s) => s.flows);
  const refreshFlows = useEngineStore((s) => s.refreshFlows);
  const saveFlowToStore = useEngineStore((s) => s.saveFlow);
  const runFlow = useEngineStore((s) => s.runFlow);
  const stopFlow = useEngineStore((s) => s.stopFlow);
  const getFlowProgress = useEngineStore((s) => s.getFlowProgress);
  const refreshStatus = useEngineStore((s) => s.refreshStatus);
  const status = useEngineStore((s) => s.status);

  const [flow, setFlow] = useState<FlowDefinition | null>(SAMPLE_FLOW);
  const [progress, setProgress] = useState<TaskGroupProgress | null>(null);
  const [sidebarTab, setSidebarTab] = useState<"palette" | "properties">(
    "palette"
  );
  const [selectedNode, setSelectedNode] = useState<Node<FlowNodeData> | null>(
    null
  );
  const reactFlowWrapper = useRef<HTMLDivElement>(null);

  const { nodes: mappedNodes, edges: mappedEdges } =
    useFlowCanvasMapping(flow);
  const [nodes, setNodes, onNodesChange] = useNodesState(mappedNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(mappedEdges);
  const { layoutNodes } = useFlowLayout();

  useEffect(() => {
    refreshFlows();
  }, [refreshFlows]);

  useEffect(() => {
    if (!flow && flows.length > 0) {
      setFlow(flows[0]);
      return;
    }
    if (flow) {
      const latest = flows.find((f) => f.id === flow.id);
      if (latest) setFlow(latest);
    }
  }, [flows, flow]);

  // Sync mapped data when flow changes
  useEffect(() => {
    setNodes(mappedNodes);
    setEdges(mappedEdges);
  }, [mappedNodes, mappedEdges, setNodes, setEdges]);

  // Handle new connections
  const onConnect: OnConnect = useCallback(
    (connection) => {
      setEdges((eds) =>
        addEdge(
          {
            ...connection,
            style: { stroke: "#6b7280", strokeWidth: 1.5 },
            animated: false,
          },
          eds
        )
      );
    },
    [setEdges]
  );

  // Handle node selection
  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      setSelectedNode(node as Node<FlowNodeData>);
      setSidebarTab("properties");
    },
    []
  );

  // Handle pane click (deselect)
  const onPaneClick = useCallback(() => {
    setSelectedNode(null);
  }, []);

  // Handle drop (drag from palette)
  const onDragOver = useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
  }, []);

  const onDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();

      const kind = event.dataTransfer.getData(
        "application/betternte-step-kind"
      ) as StepKindType;
      if (!kind) return;

      const wrapper = reactFlowWrapper.current;
      if (!wrapper) return;

      const bounds = wrapper.getBoundingClientRect();
      const position = {
        x: event.clientX - bounds.left - 90,
        y: event.clientY - bounds.top - 30,
      };

      // Generate a unique step ID
      const existingIds = flow ? Object.keys(flow.steps) : [];
      let counter = 1;
      let stepId = `${kind}_${counter}`;
      while (existingIds.includes(stepId)) {
        counter++;
        stepId = `${kind}_${counter}`;
      }

      // Create default StepKind
      let stepKind: StepKind;
      switch (kind) {
        case "script":
          stepKind = { type: "script", script: "new_script" };
          break;
        case "click":
          stepKind = { type: "click", x: 0, y: 0 };
          break;
        case "swipe":
          stepKind = {
            type: "swipe",
            x1: 0,
            y1: 0,
            x2: 100,
            y2: 100,
            duration_ms: 300,
          };
          break;
        case "key_press":
          stepKind = { type: "key_press", key: "F" };
          break;
        case "wait":
          stepKind = { type: "wait", ms: 1000 };
          break;
        case "flow":
          stepKind = { type: "flow", flow: "sub_flow" };
          break;
        case "group":
          stepKind = { type: "group", group: "sub_group" };
          break;
        case "set_variable":
          stepKind = { type: "set_variable", key: "var", value: 0 };
          break;
        case "none":
          stepKind = { type: "none" };
          break;
        default:
          stepKind = { type: "none" };
      }

      const newNode: Node<FlowNodeData> = {
        id: stepId,
        type: "flow-node",
        position,
        data: {
          stepId,
          step: {
            kind: stepKind,
            input: {},
            output: {},
            transitions: [],
            max_retries: 0,
          },
          isEntry: false,
          label: stepId,
        },
      };

      setNodes((nds) => [...nds, newNode]);

      // Also update flow state
      if (flow) {
        setFlow({
          ...flow,
          steps: {
            ...flow.steps,
            [stepId]: {
              kind: stepKind,
              input: {},
              output: {},
              transitions: [],
              max_retries: 0,
            },
          },
        });
      }
    },
    [flow, setNodes]
  );

  // Auto-layout
  const handleLayout = useCallback(() => {
    const layouted = layoutNodes(nodes, edges, "LR");
    setNodes(layouted);
  }, [nodes, edges, layoutNodes, setNodes]);

  // Delete node
  const handleDeleteNode = useCallback(
    (id: string) => {
      setNodes((nds) => nds.filter((n) => n.id !== id));
      setEdges((eds) =>
        eds.filter((e) => e.source !== id && e.target !== id)
      );
      setSelectedNode(null);

      if (flow) {
        const newSteps = { ...flow.steps };
        delete newSteps[id];
        setFlow({ ...flow, steps: newSteps });
      }
    },
    [flow, setNodes, setEdges]
  );

  // Update node data
  const handleUpdateNode = useCallback(
    (id: string, data: Partial<FlowNodeData>) => {
      setNodes((nds) =>
        nds.map((n) =>
          n.id === id
            ? { ...n, data: { ...n.data, ...data } as FlowNodeData }
            : n
        )
      );
    },
    [setNodes]
  );

  // Load flow from JSON file
  const handleLoadFlow = useCallback(() => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (!file) return;
      const reader = new FileReader();
      reader.onload = (ev) => {
        try {
          const parsed = JSON.parse(ev.target?.result as string);
          setFlow(parsed as FlowDefinition);
        } catch (err) {
          console.error("Failed to parse flow JSON:", err);
        }
      };
      reader.readAsText(file);
    };
    input.click();
  }, []);

  // Save flow to JSON
  const handleSaveFlow = useCallback(async () => {
    if (!flow) return;
    await saveFlowToStore(flow);
  }, [flow, saveFlowToStore]);

  // Run flow
  const handleRunFlow = useCallback(async () => {
    if (!flow) return;
    await saveFlowToStore(flow);
    await runFlow(flow.id);
  }, [flow, runFlow, saveFlowToStore]);

  const handleStopFlow = useCallback(async () => {
    if (!flow) return;
    await stopFlow(flow.id);
    setProgress(null);
  }, [flow, stopFlow]);

  useEffect(() => {
    if (!flow || status.task !== flow.id || status.task_type !== "flow") {
      setProgress(null);
      return;
    }
    const timer = setInterval(async () => {
      const p = await getFlowProgress(flow.id);
      setProgress(p);
      if (!p) {
        await refreshStatus();
      }
    }, UI_POLL_FAST_MS);
    return () => clearInterval(timer);
  }, [flow, status.task, status.task_type, getFlowProgress, refreshStatus]);

  return (
    <div className="flex h-full">
      {/* Left sidebar — palette / properties */}
      <div className="w-56 border-r border-border-subtle bg-surface/50 flex flex-col shrink-0">
        {/* Tab switcher */}
        <div className="flex border-b border-border-subtle">
          <button
            onClick={() => setSidebarTab("palette")}
            className={cn(
              "flex-1 px-3 py-2 text-xs font-medium transition-colors",
              sidebarTab === "palette"
                ? "text-primary border-b-2 border-primary"
                : "text-foreground-tertiary hover:text-foreground"
            )}
          >
            节点
          </button>
          <button
            onClick={() => setSidebarTab("properties")}
            className={cn(
              "flex-1 px-3 py-2 text-xs font-medium transition-colors",
              sidebarTab === "properties"
                ? "text-primary border-b-2 border-primary"
                : "text-foreground-tertiary hover:text-foreground"
            )}
          >
            属性
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-3">
          {sidebarTab === "palette" ? (
            <StepPalette />
          ) : (
            <StepPropertyPanel
              node={selectedNode}
              onUpdate={handleUpdateNode}
              onDelete={handleDeleteNode}
              onClose={() => {
                setSelectedNode(null);
                setSidebarTab("palette");
              }}
            />
          )}
        </div>
      </div>

      {/* Main canvas area */}
      <div className="flex-1 flex flex-col">
        {/* Toolbar */}
        <div className="flex items-center gap-2 px-4 py-2 border-b border-border-subtle bg-surface/30">
          <div className="flex items-center gap-1.5 mr-4">
            <GitBranch className="w-4 h-4 text-foreground-tertiary" />
            <span className="text-sm font-semibold text-foreground">
              {flow?.name || "未命名工作流"}
            </span>
            {flow && (
              <span className="text-xs text-foreground-tertiary font-mono">
                v{flow.version}
              </span>
            )}
          </div>

          <div className="flex-1" />

          <button
            onClick={handleLayout}
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs text-foreground-secondary hover:bg-surface-hover transition-colors"
            title="自动布局"
          >
            <LayoutGrid className="w-3.5 h-3.5" />
            布局
          </button>

          <button
            onClick={handleLoadFlow}
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs text-foreground-secondary hover:bg-surface-hover transition-colors"
            title="加载工作流"
          >
            <FolderOpen className="w-3.5 h-3.5" />
            加载
          </button>

          <button
            onClick={handleSaveFlow}
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs text-foreground-secondary hover:bg-surface-hover transition-colors"
            title="保存工作流"
          >
            <Save className="w-3.5 h-3.5" />
            保存
          </button>

          {status.task === flow?.id && status.task_type === "flow" ? (
            <button
              onClick={handleStopFlow}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium bg-destructive text-destructive-foreground hover:bg-destructive/90 transition-colors"
            >
              <X className="w-3.5 h-3.5" />
              停止
            </button>
          ) : (
            <button
              onClick={handleRunFlow}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium bg-primary text-primary-foreground hover:bg-primary-hover transition-colors"
            >
              <Play className="w-3.5 h-3.5" />
              运行
            </button>
          )}
        </div>

        {/* React Flow canvas */}
        <div ref={reactFlowWrapper} className="flex-1">
          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onNodeClick={onNodeClick}
            onPaneClick={onPaneClick}
            onDragOver={onDragOver}
            onDrop={onDrop}
            nodeTypes={nodeTypes}
            fitView
            snapToGrid
            snapGrid={[20, 20]}
            defaultEdgeOptions={{
              style: { stroke: "#6b7280", strokeWidth: 1.5 },
            }}
            proOptions={{ hideAttribution: true }}
          >
            <Background gap={20} size={1} />
            <Controls />
            <MiniMap
              nodeColor={(node) => {
                const data = node.data as FlowNodeData;
                return STEP_KIND_META[data?.step?.kind?.type]?.color ?? "#6b7280";
              }}
              maskColor="rgba(0,0,0,0.6)"
            />
            <Panel position="top-left">
              <div className="text-xs text-foreground-tertiary bg-background/80 backdrop-blur px-2 py-1 rounded">
                拖拽左侧节点到画布 · 点击节点查看属性
                {progress ? ` · 进度 ${progress.completed}/${progress.total}` : ""}
              </div>
            </Panel>
          </ReactFlow>
        </div>
      </div>
    </div>
  );
}
