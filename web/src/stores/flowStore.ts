import { create } from 'zustand';
import type { Node, Edge } from '@xyflow/react';
import { toFlowJSON, fromFlowJSON } from '../utils/flowConverter';

export interface LogEntry {
  level: 'info' | 'warn' | 'error' | 'debug';
  msg: string;
  ts: number;
  stepId?: string;
}

export interface FlowStore {
  nodes: Node[];
  edges: Edge[];
  selectedNode: Node | null;
  flowName: string;
  logs: LogEntry[];
  connectionStatus: 'disconnected' | 'connecting' | 'connected';
  sessionId: string | null;
  executionStatus: 'idle' | 'running' | 'completed' | 'error';
  runningStepId: string | null;

  setNodes: (nodes: Node[]) => void;
  setEdges: (edges: Edge[]) => void;
  onNodesChange: (changes: { type: string; id: string; [key: string]: unknown }[]) => void;
  onEdgesChange: (changes: { type: string; id: string; [key: string]: unknown }[]) => void;
  addNode: (node: Node) => void;
  removeNode: (id: string) => void;
  updateNode: (id: string, data: Record<string, unknown>) => void;
  addEdge: (edge: Edge) => void;
  removeEdge: (id: string) => void;
  setSelectedNode: (node: Node | null) => void;
  setFlowName: (name: string) => void;
  addLog: (entry: LogEntry) => void;
  clearLogs: () => void;
  setConnectionStatus: (status: 'disconnected' | 'connecting' | 'connected') => void;
  setSessionId: (id: string | null) => void;
  setExecutionStatus: (status: 'idle' | 'running' | 'completed' | 'error') => void;
  setRunningStep: (stepId: string | null) => void;
  exportFlow: () => Record<string, unknown>;
  importFlow: (json: Record<string, unknown>) => void;
}

export const useFlowStore = create<FlowStore>((set, get) => ({
  nodes: [],
  edges: [],
  selectedNode: null,
  flowName: 'Untitled Flow',
  logs: [],
  connectionStatus: 'disconnected',
  sessionId: null,
  executionStatus: 'idle',
  runningStepId: null,

  setNodes: (nodes) => set({ nodes }),
  setEdges: (edges) => set({ edges }),

  onNodesChange: (changes) => {
    set((state) => {
      const nextNodes = [...state.nodes];
      for (const change of changes) {
        if (change.type === 'remove') {
          const idx = nextNodes.findIndex((n) => n.id === change.id);
          if (idx !== -1) nextNodes.splice(idx, 1);
        } else if (change.type === 'position' && 'position' in change) {
          const node = nextNodes.find((n) => n.id === change.id);
          if (node && change.position) {
            node.position = change.position as { x: number; y: number };
          }
        } else if (change.type === 'select') {
          // handled by selectedNode
        }
      }
      return { nodes: nextNodes };
    });
  },

  onEdgesChange: (changes) => {
    set((state) => {
      const nextEdges = [...state.edges];
      for (const change of changes) {
        if (change.type === 'remove') {
          const idx = nextEdges.findIndex((e) => e.id === change.id);
          if (idx !== -1) nextEdges.splice(idx, 1);
        }
      }
      return { edges: nextEdges };
    });
  },

  addNode: (node) => set((state) => ({ nodes: [...state.nodes, node] })),
  removeNode: (id) =>
    set((state) => ({
      nodes: state.nodes.filter((n) => n.id !== id),
      edges: state.edges.filter((e) => e.source !== id && e.target !== id),
      selectedNode: state.selectedNode?.id === id ? null : state.selectedNode,
    })),
  updateNode: (id, data) =>
    set((state) => ({
      nodes: state.nodes.map((n) =>
        n.id === id ? { ...n, data: { ...n.data, ...data } } : n
      ),
      selectedNode:
        state.selectedNode?.id === id
          ? { ...state.selectedNode, data: { ...state.selectedNode.data, ...data } }
          : state.selectedNode,
    })),
  addEdge: (edge) => set((state) => ({ edges: [...state.edges, edge] })),
  removeEdge: (id) =>
    set((state) => ({ edges: state.edges.filter((e) => e.id !== id) })),
  setSelectedNode: (node) => set({ selectedNode: node }),
  setFlowName: (name) => set({ flowName: name }),
  addLog: (entry) =>
    set((state) => ({ logs: [...state.logs.slice(-499), entry] })),
  clearLogs: () => set({ logs: [] }),
  setConnectionStatus: (status) => set({ connectionStatus: status }),
  setSessionId: (id) => set({ sessionId: id }),
  setExecutionStatus: (status) => set({ executionStatus: status }),
  setRunningStep: (stepId) => set({ runningStepId: stepId }),

  exportFlow: () => {
    const { nodes, edges, flowName } = get();
    return toFlowJSON(nodes, edges, flowName);
  },

  importFlow: (json) => {
    const { nodes, edges } = fromFlowJSON(json);
    set({ nodes, edges, flowName: (json.name as string) || 'Imported Flow' });
  },
}));
