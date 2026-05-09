import { useEffect, useRef, useState } from 'react';
import { useFlowStore } from '../../stores/flowStore';

const LEVEL_COLORS: Record<string, string> = {
  info: '#e0e0e0',
  warn: '#fbbf24',
  error: '#ef4444',
  debug: '#6b7280',
};

export function LogPanel() {
  const logs = useFlowStore((s) => s.logs);
  const clearLogs = useFlowStore((s) => s.clearLogs);
  const [filter, setFilter] = useState<string>('all');
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  const filtered = filter === 'all' ? logs : logs.filter((l) => l.level === filter);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '4px 8px',
          background: '#16213e',
          borderBottom: '1px solid #0f3460',
          flexShrink: 0,
        }}
      >
        <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
          <span style={{ fontSize: '11px', color: '#9ca3af', fontWeight: 600 }}>LOGS</span>
          {(['all', 'info', 'warn', 'error', 'debug'] as const).map((level) => (
            <button
              key={level}
              onClick={() => setFilter(level)}
              style={{
                padding: '1px 6px',
                fontSize: '10px',
                background: filter === level ? '#0f3460' : 'transparent',
                border: '1px solid #374151',
                borderRadius: '3px',
                color: filter === level ? '#e0e0e0' : '#6b7280',
                cursor: 'pointer',
                textTransform: 'uppercase',
              }}
            >
              {level}
            </button>
          ))}
        </div>
        <button
          onClick={clearLogs}
          style={{
            padding: '1px 6px',
            fontSize: '10px',
            background: 'transparent',
            border: '1px solid #374151',
            borderRadius: '3px',
            color: '#6b7280',
            cursor: 'pointer',
          }}
        >
          Clear
        </button>
      </div>
      <div
        ref={scrollRef}
        style={{
          flex: 1,
          overflowY: 'auto',
          padding: '4px 8px',
          fontFamily: 'ui-monospace, Consolas, monospace',
          fontSize: '11px',
          lineHeight: '1.6',
        }}
      >
        {filtered.length === 0 && (
          <div style={{ color: '#4a5568', textAlign: 'center', padding: '12px' }}>
            No logs yet
          </div>
        )}
        {filtered.map((entry, i) => (
          <div key={i} style={{ display: 'flex', gap: '8px' }}>
            <span style={{ color: '#4a5568', flexShrink: 0 }}>
              {new Date(entry.ts).toLocaleTimeString()}
            </span>
            <span
              style={{
                color: LEVEL_COLORS[entry.level] || '#e0e0e0',
                flexShrink: 0,
                textTransform: 'uppercase',
                width: '36px',
              }}
            >
              {entry.level}
            </span>
            {entry.stepId && (
              <span style={{ color: '#3b82f6', flexShrink: 0 }}>[{entry.stepId}]</span>
            )}
            <span style={{ color: LEVEL_COLORS[entry.level] || '#e0e0e0' }}>{entry.msg}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
