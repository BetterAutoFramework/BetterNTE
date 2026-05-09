import { memo } from 'react';
import { Handle, Position } from '@xyflow/react';

function StartNodeComponent() {
  return (
    <div
      style={{
        background: 'rgba(46, 204, 113, 0.15)',
        border: '2px solid #2ecc71',
        borderRadius: '50%',
        width: '60px',
        height: '60px',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        boxShadow: '0 0 8px rgba(46, 204, 113, 0.3)',
      }}
    >
      <span style={{ fontSize: '14px', fontWeight: 700, color: '#2ecc71' }}>▶</span>
      <Handle
        type="source"
        position={Position.Right}
        style={{
          background: '#2ecc71',
          width: '8px',
          height: '8px',
          border: '2px solid #1a1a2e',
        }}
      />
    </div>
  );
}

export const StartNode = memo(StartNodeComponent);
