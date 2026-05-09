import { useCallback, useMemo } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  BackgroundVariant,
  type Node,
  type Edge,
  type OnConnect,
  type OnNodeClick,
  type NodeTypes,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { useFlowStore } from './stores/flowStore';
import { StepNode } from './components/nodes/StepNode';
import { StartNode } from './components/nodes/StartNode';
import { Toolbar } from './components/Toolbar';
import { StatusBar } from './components/StatusBar';
import { NodeConfigPanel } from './components/panels/NodeConfigPanel';
import { LogPanel } from './components/panels/LogPanel';

const nodeTypes: NodeTypes = {
  step: StepNode,
  start: StartNode,
};

export default function App() {
  const nodes = useFlowStore((s) => s.nodes);
  const edges = useFlowStore((s) => s.edges);
  const onNodesChange = useFlowStore((s) => s.onNodesChange);
  const onEdgesChange = useFlowStore((s) => s.onEdgesChange);
  const addEdge = useFlowStore((s) => s.addEdge);
  const addNode = useFlowStore((s) => s.addNode);
  const setSelectedNode = useFlowStore((s) => s.setSelectedNode);
  const selectedNode = useFlowStore((s) => s.selectedNode);

  const onConnect: OnConnect = useCallback(
    (params) => {
      addEdge({
        id: `${params.source}->${params.target}`,
        source: params.source!,
        target: params.target!,
        type: 'default',
        data: { condition: { type: 'always' }, priority: 0 },
      });
    },
    [addEdge]
  );

  const onNodeClick: OnNodeClick = useCallback(
    (_event, node) => {
      setSelectedNode(node);
    },
    [setSelectedNode]
  );

  const onPaneClick = useCallback(() => {
    setSelectedNode(null);
  }, [setSelectedNode]);

  const onDragOver = useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  }, []);

  const onDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();
      const stepType = event.dataTransfer.getData('application/betternte-step');
      if (!stepType) return;

      const bounds = (event.target as HTMLElement).getBoundingClientRect();
      const position = {
        x: event.clientX - bounds.left - 70,
        y: event.clientY - bounds.top - 20,
      };

      const id = `step_${Date.now()}`;
      const newNode: Node = {
        id,
        type: 'step',
        position,
        data: {
          label: id,
          kind: { type: stepType },
          input: {},
          output: {},
          timeout_ms: undefined,
          max_retries: 0,
          on_error: undefined,
          transitions: [],
        },
      };
      addNode(newNode);
    },
    [addNode]
  );

  // Memoize the default edge options
  const defaultEdgeOptions = useMemo(
    () => ({
      type: 'default' as const,
      style: { stroke: '#4a5568', strokeWidth: 2 },
      animated: false,
    }),
    []
  );

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100vh', background: '#1a1a2e' }}>
      <Toolbar />

      <div style={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
        {/* Canvas */}
        <div style={{ flex: 1, position: 'relative' }}>
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
            defaultEdgeOptions={defaultEdgeOptions}
            fitView
            style={{ background: '#1a1a2e' }}
            proOptions={{ hideAttribution: true }}
          >
            <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="#2a2a4a" />
            <Controls
              position="bottom-left"
              style={{ marginBottom: '200px' }}
            />
            <MiniMap
              nodeColor={(node) => {
                if (node.type === 'start') return '#2ecc71';
                const kind = (node.data as Record<string, unknown>)?.kind as { type: string } | undefined;
                const colors: Record<string, string> = {
                  script: '#9b59b6',
                  click: '#2ecc71',
                  swipe: '#3498db',
                  key_press: '#f1c40f',
                  wait: '#95a5a6',
                  set_variable: '#e67e22',
                  flow: '#1abc9c',
                  group: '#16a085',
                };
                return colors[kind?.type || ''] || '#7f8c8d';
              }}
              style={{
                background: '#16213e',
                marginBottom: '200px',
              }}
              position="bottom-right"
            />
          </ReactFlow>

          {/* Step palette overlay */}
          <StepPalette />
        </div>

        {/* Right sidebar */}
        {selectedNode && (
          <div
            style={{
              width: '280px',
              background: '#16213e',
              borderLeft: '1px solid #0f3460',
              overflowY: 'auto',
              flexShrink: 0,
            }}
          >
            <NodeConfigPanel />
          </div>
        )}
      </div>

      {/* Bottom panel */}
      <div
        style={{
          height: '180px',
          background: '#16213e',
          borderTop: '1px solid #0f3460',
          flexShrink: 0,
        }}
      >
        <LogPanel />
      </div>

      <StatusBar />
    </div>
  );
}

// Step palette — draggable step types
function StepPalette() {
  const stepTypes = [
    { type: 'script', label: 'Script', color: '#9b59b6', icon: '⟨/⟩' },
    { type: 'click', label: 'Click', color: '#2ecc71', icon: '🖱' },
    { type: 'swipe', label: 'Swipe', color: '#3498db', icon: '👆' },
    { type: 'key_press', label: 'Key', color: '#f1c40f', icon: '⌨' },
    { type: 'wait', label: 'Wait', color: '#95a5a6', icon: '⏱' },
    { type: 'set_variable', label: 'Var', color: '#e67e22', icon: 'x=' },
    { type: 'flow', label: 'Flow', color: '#1abc9c', icon: '▸' },
    { type: 'group', label: 'Group', color: '#16a085', icon: '☰' },
  ];

  return (
    <div
      style={{
        position: 'absolute',
        top: '8px',
        left: '8px',
        display: 'flex',
        flexDirection: 'column',
        gap: '4px',
        background: '#16213e',
        borderRadius: '8px',
        padding: '6px',
        border: '1px solid #0f3460',
        zIndex: 10,
      }}
    >
      <span style={{ fontSize: '10px', color: '#6b7280', textAlign: 'center', marginBottom: '2px' }}>
        Drag to add
      </span>
      {stepTypes.map((s) => (
        <div
          key={s.type}
          draggable
          onDragStart={(e) => {
            e.dataTransfer.setData('application/betternte-step', s.type);
            e.dataTransfer.effectAllowed = 'move';
          }}
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '6px',
            padding: '4px 8px',
            background: `${s.color}20`,
            border: `1px solid ${s.color}40`,
            borderRadius: '4px',
            cursor: 'grab',
            fontSize: '11px',
            color: s.color,
            userSelect: 'none',
            minWidth: '80px',
          }}
        >
          <span style={{ fontSize: '12px', width: '16px', textAlign: 'center' }}>{s.icon}</span>
          <span>{s.label}</span>
        </div>
      ))}
    </div>
  );
}
