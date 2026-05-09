import { memo } from 'react';
import { Handle, Position } from '@xyflow/react';
import { useFlowStore } from '../../stores/flowStore';

interface StepKind {
  type: string;
  [key: string]: unknown;
}

interface StepNodeData {
  label: string;
  kind: StepKind;
  input?: Record<string, string>;
  output?: Record<string, string>;
  timeout_ms?: number;
  max_retries?: number;
  on_error?: string;
  transitions?: unknown[];
}

const STEP_STYLES: Record<string, { color: string; icon: string; bg: string }> = {
  script: { color: '#9b59b6', icon: '⟨/⟩', bg: 'rgba(155, 89, 182, 0.15)' },
  click: { color: '#2ecc71', icon: '🖱', bg: 'rgba(46, 204, 113, 0.15)' },
  swipe: { color: '#3498db', icon: '👆', bg: 'rgba(52, 152, 219, 0.15)' },
  key_press: { color: '#f1c40f', icon: '⌨', bg: 'rgba(241, 196, 15, 0.15)' },
  wait: { color: '#95a5a6', icon: '⏱', bg: 'rgba(149, 165, 166, 0.15)' },
  set_variable: { color: '#e67e22', icon: 'x=', bg: 'rgba(230, 126, 34, 0.15)' },
  flow: { color: '#1abc9c', icon: '▸', bg: 'rgba(26, 188, 156, 0.15)' },
  group: { color: '#16a085', icon: '☰', bg: 'rgba(22, 160, 133, 0.15)' },
  none: { color: '#7f8c8d', icon: '○', bg: 'rgba(127, 140, 141, 0.15)' },
};

function getStepSummary(kind: StepKind): string {
  switch (kind.type) {
    case 'script':
      return (kind.script as string) || '';
    case 'click':
      return `(${kind.x}, ${kind.y})`;
    case 'swipe':
      return `(${kind.x1},${kind.y1})→(${kind.x2},${kind.y2})`;
    case 'key_press':
      return (kind.key as string) || '';
    case 'wait':
      return `${kind.ms}ms`;
    case 'set_variable':
      return `${kind.key} = ${kind.value}`;
    case 'flow':
      return (kind.flow as string) || '';
    case 'group':
      return (kind.group as string) || '';
    default:
      return '';
  }
}

function StepNodeComponent({ data, id }: { data: Record<string, unknown>; id: string }) {
  const stepData = data as unknown as StepNodeData;
  const kind = stepData.kind || { type: 'none' };
  const style = STEP_STYLES[kind.type] || STEP_STYLES.none;
  const summary = getStepSummary(kind);
  const runningStepId = useFlowStore((s) => s.runningStepId);
  const selectedNode = useFlowStore((s) => s.selectedNode);
  const isRunning = runningStepId === stepData.label || runningStepId === id;
  const isSelected = selectedNode?.id === id;

  return (
    <div
      style={{
        background: style.bg,
        border: `2px solid ${isRunning ? '#fff' : isSelected ? style.color : 'rgba(255,255,255,0.2)'}`,
        borderRadius: '8px',
        padding: '8px 12px',
        minWidth: '140px',
        boxShadow: isRunning
          ? `0 0 12px ${style.color}, 0 0 24px ${style.color}40`
          : isSelected
          ? `0 0 8px ${style.color}60`
          : 'none',
        transition: 'all 0.2s ease',
      }}
    >
      <Handle
        type="target"
        position={Position.Left}
        style={{
          background: style.color,
          width: '8px',
          height: '8px',
          border: '2px solid #1a1a2e',
        }}
      />
      <div style={{ display: 'flex', alignItems: 'center', gap: '6px', marginBottom: '2px' }}>
        <span style={{ fontSize: '14px' }}>{style.icon}</span>
        <span
          style={{
            fontSize: '12px',
            fontWeight: 600,
            color: style.color,
            textTransform: 'uppercase',
            letterSpacing: '0.5px',
          }}
        >
          {kind.type.replace('_', ' ')}
        </span>
      </div>
      <div style={{ fontSize: '13px', fontWeight: 500, color: '#e0e0e0' }}>
        {stepData.label}
      </div>
      {summary && (
        <div
          style={{
            fontSize: '11px',
            color: '#9ca3af',
            marginTop: '2px',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            maxWidth: '150px',
          }}
        >
          {summary}
        </div>
      )}
      <Handle
        type="source"
        position={Position.Right}
        style={{
          background: style.color,
          width: '8px',
          height: '8px',
          border: '2px solid #1a1a2e',
        }}
      />
    </div>
  );
}

export const StepNode = memo(StepNodeComponent);
