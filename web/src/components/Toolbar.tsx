import { useState, useRef } from 'react';
import { useFlowStore } from '../stores/flowStore';
import { useRelay } from '../hooks/useRelay';

export function Toolbar() {
  const flowName = useFlowStore((s) => s.flowName);
  const setFlowName = useFlowStore((s) => s.setFlowName);
  const connectionStatus = useFlowStore((s) => s.connectionStatus);
  const exportFlow = useFlowStore((s) => s.exportFlow);
  const importFlow = useFlowStore((s) => s.importFlow);
  const addLog = useFlowStore((s) => s.addLog);
  const executionStatus = useFlowStore((s) => s.executionStatus);
  const { connect, disconnect, sendMessage, isConnected } = useRelay();

  const [sessionId, setSessionId] = useState('');
  const fileInputRef = useRef<HTMLInputElement>(null);

  const statusColor =
    connectionStatus === 'connected'
      ? '#2ecc71'
      : connectionStatus === 'connecting'
      ? '#f1c40f'
      : '#ef4444';

  const handleConnect = () => {
    if (isConnected) {
      disconnect();
    } else if (sessionId.trim()) {
      connect(sessionId.trim());
    }
  };

  const handleRun = () => {
    const flow = exportFlow();
    sendMessage('run_flow', { flow });
    addLog({ level: 'info', msg: 'Flow sent to client', ts: Date.now() });
  };

  const handleStop = () => {
    sendMessage('stop_flow', {});
    addLog({ level: 'info', msg: 'Stop signal sent', ts: Date.now() });
  };

  const handleExport = () => {
    const flow = exportFlow();
    const json = JSON.stringify(flow, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${flowName.replace(/\s+/g, '_')}.json`;
    a.click();
    URL.revokeObjectURL(url);
    addLog({ level: 'info', msg: 'Flow exported', ts: Date.now() });
  };

  const handleImport = () => {
    fileInputRef.current?.click();
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const json = JSON.parse(reader.result as string);
        importFlow(json);
        addLog({ level: 'info', msg: `Imported: ${file.name}`, ts: Date.now() });
      } catch (err) {
        addLog({ level: 'error', msg: `Import failed: ${err}`, ts: Date.now() });
      }
    };
    reader.readAsText(file);
    e.target.value = '';
  };

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: '10px',
        padding: '6px 12px',
        background: '#16213e',
        borderBottom: '1px solid #0f3460',
        flexShrink: 0,
      }}
    >
      {/* Flow name */}
      <input
        value={flowName}
        onChange={(e) => setFlowName(e.target.value)}
        style={{
          background: '#1a1a2e',
          border: '1px solid #374151',
          borderRadius: '4px',
          padding: '4px 8px',
          color: '#e0e0e0',
          fontSize: '13px',
          width: '160px',
          outline: 'none',
        }}
      />

      <div style={{ width: '1px', height: '20px', background: '#374151' }} />

      {/* Session connection */}
      <span style={{ fontSize: '11px', color: '#9ca3af' }}>Session:</span>
      <input
        value={sessionId}
        onChange={(e) => setSessionId(e.target.value)}
        placeholder="session-id"
        style={{
          background: '#1a1a2e',
          border: '1px solid #374151',
          borderRadius: '4px',
          padding: '4px 8px',
          color: '#e0e0e0',
          fontSize: '12px',
          width: '120px',
          outline: 'none',
        }}
      />
      <button onClick={handleConnect} style={btnStyle}>
        {isConnected ? 'Disconnect' : 'Connect'}
      </button>
      <span
        style={{
          width: '8px',
          height: '8px',
          borderRadius: '50%',
          background: statusColor,
          boxShadow: `0 0 4px ${statusColor}`,
        }}
      />

      <div style={{ width: '1px', height: '20px', background: '#374151' }} />

      {/* Run / Stop */}
      <button
        onClick={handleRun}
        disabled={!isConnected || executionStatus === 'running'}
        style={{
          ...btnStyle,
          opacity: !isConnected || executionStatus === 'running' ? 0.5 : 1,
          background: '#065f46',
          borderColor: '#059669',
        }}
      >
        ▶ Run
      </button>
      <button
        onClick={handleStop}
        disabled={executionStatus !== 'running'}
        style={{
          ...btnStyle,
          opacity: executionStatus !== 'running' ? 0.5 : 1,
          background: '#7f1d1d',
          borderColor: '#991b1b',
        }}
      >
        ■ Stop
      </button>

      <div style={{ flex: 1 }} />

      {/* Export / Import */}
      <button onClick={handleExport} style={btnStyle}>
        Export
      </button>
      <button onClick={handleImport} style={btnStyle}>
        Import
      </button>
      <input
        ref={fileInputRef}
        type="file"
        accept=".json"
        onChange={handleFileChange}
        style={{ display: 'none' }}
      />
    </div>
  );
}

const btnStyle: React.CSSProperties = {
  padding: '4px 10px',
  fontSize: '12px',
  background: '#0f3460',
  border: '1px solid #374151',
  borderRadius: '4px',
  color: '#e0e0e0',
  cursor: 'pointer',
  whiteSpace: 'nowrap',
};
