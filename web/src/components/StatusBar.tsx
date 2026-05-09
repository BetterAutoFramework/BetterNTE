import { useFlowStore } from '../stores/flowStore';

const STATUS_LABELS: Record<string, string> = {
  disconnected: '● Disconnected',
  connecting: '● Connecting…',
  connected: '● Connected',
};

const STATUS_COLORS: Record<string, string> = {
  disconnected: '#ef4444',
  connecting: '#f1c40f',
  connected: '#2ecc71',
};

const EXEC_LABELS: Record<string, string> = {
  idle: 'Idle',
  running: 'Running…',
  completed: 'Completed',
  error: 'Error',
};

const EXEC_COLORS: Record<string, string> = {
  idle: '#6b7280',
  running: '#3b82f6',
  completed: '#2ecc71',
  error: '#ef4444',
};

export function StatusBar() {
  const connectionStatus = useFlowStore((s) => s.connectionStatus);
  const executionStatus = useFlowStore((s) => s.executionStatus);
  const runningStepId = useFlowStore((s) => s.runningStepId);
  const nodes = useFlowStore((s) => s.nodes);
  const edges = useFlowStore((s) => s.edges);

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: '16px',
        padding: '3px 12px',
        background: '#0f3460',
        borderTop: '1px solid #1a1a2e',
        fontSize: '11px',
        color: '#9ca3af',
        flexShrink: 0,
      }}
    >
      <span style={{ color: STATUS_COLORS[connectionStatus] }}>
        {STATUS_LABELS[connectionStatus]}
      </span>
      <span style={{ color: EXEC_COLORS[executionStatus] }}>
        {EXEC_LABELS[executionStatus]}
      </span>
      {runningStepId && (
        <span style={{ color: '#3b82f6' }}>Step: {runningStepId}</span>
      )}
      <div style={{ flex: 1 }} />
      <span>Nodes: {nodes.length}</span>
      <span>Edges: {edges.length}</span>
    </div>
  );
}
