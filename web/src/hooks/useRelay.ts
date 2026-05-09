import { useCallback, useEffect, useRef } from 'react';
import { useFlowStore } from '../stores/flowStore';

export function useRelay() {
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const sessionIdRef = useRef<string | null>(null);

  const {
    setConnectionStatus,
    setSessionId,
    addLog,
    setExecutionStatus,
    setRunningStep,
  } = useFlowStore();

  const cleanup = useCallback(() => {
    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
  }, []);

  const connect = useCallback(
    (sessionId: string) => {
      cleanup();
      sessionIdRef.current = sessionId;
      setConnectionStatus('connecting');
      setSessionId(sessionId);

      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      // In dev, Vite proxies /ws -> ws://localhost:9280
      // In production, connect directly
      const isDev = import.meta.env.DEV;
      const wsUrl = isDev
        ? `${protocol}//${window.location.host}/ws`
        : `${protocol}//${window.location.hostname}:9280`;

      const ws = new WebSocket(wsUrl);
      wsRef.current = ws;

      ws.onopen = () => {
        // Send join message as browser
        ws.send(
          JSON.stringify({
            type: 'join_browser',
            session_id: sessionId,
          })
        );
        setConnectionStatus('connected');
        addLog({
          level: 'info',
          msg: `Connected to session ${sessionId}`,
          ts: Date.now(),
        });
      };

      ws.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data);
          handleMessage(msg);
        } catch {
          addLog({
            level: 'debug',
            msg: `Raw message: ${event.data}`,
            ts: Date.now(),
          });
        }
      };

      ws.onclose = () => {
        setConnectionStatus('disconnected');
        addLog({
          level: 'warn',
          msg: 'Disconnected from relay',
          ts: Date.now(),
        });
        // Auto-reconnect after 3s if we have a session
        if (sessionIdRef.current) {
          reconnectTimerRef.current = setTimeout(() => {
            if (sessionIdRef.current) {
              connect(sessionIdRef.current);
            }
          }, 3000);
        }
      };

      ws.onerror = () => {
        addLog({
          level: 'error',
          msg: 'WebSocket error',
          ts: Date.now(),
        });
      };
    },
    [cleanup, setConnectionStatus, setSessionId, addLog]
  );

  const handleMessage = useCallback(
    (msg: { type: string; payload?: Record<string, unknown>; msg?: string }) => {
      switch (msg.type) {
        case 'execution_started':
          setExecutionStatus('running');
          addLog({ level: 'info', msg: 'Flow execution started', ts: Date.now() });
          break;
        case 'execution_completed':
          setExecutionStatus('completed');
          setRunningStep(null);
          addLog({ level: 'info', msg: 'Flow execution completed', ts: Date.now() });
          break;
        case 'execution_error':
          setExecutionStatus('error');
          setRunningStep(null);
          addLog({
            level: 'error',
            msg: `Execution error: ${msg.msg || 'unknown'}`,
            ts: Date.now(),
          });
          break;
        case 'step_started': {
          const stepId = (msg.payload as Record<string, unknown>)?.step_id as string;
          setRunningStep(stepId);
          addLog({
            level: 'info',
            msg: `Step started: ${stepId}`,
            ts: Date.now(),
            stepId,
          });
          break;
        }
        case 'step_completed': {
          const stepId = (msg.payload as Record<string, unknown>)?.step_id as string;
          setRunningStep(null);
          addLog({
            level: 'info',
            msg: `Step completed: ${stepId}`,
            ts: Date.now(),
            stepId,
          });
          break;
        }
        case 'step_error': {
          const stepId = (msg.payload as Record<string, unknown>)?.step_id as string;
          const error = (msg.payload as Record<string, unknown>)?.error as string;
          setRunningStep(null);
          addLog({
            level: 'error',
            msg: `Step error [${stepId}]: ${error}`,
            ts: Date.now(),
            stepId,
          });
          break;
        }
        case 'log':
          addLog({
            level: ((msg.payload as Record<string, unknown>)?.level as string as 'info' | 'warn' | 'error' | 'debug') || 'info',
            msg: (msg.payload as Record<string, unknown>)?.msg as string || msg.msg || '',
            ts: Date.now(),
            stepId: (msg.payload as Record<string, unknown>)?.step_id as string | undefined,
          });
          break;
        default:
          addLog({
            level: 'debug',
            msg: `Unknown message type: ${msg.type}`,
            ts: Date.now(),
          });
      }
    },
    [addLog, setExecutionStatus, setRunningStep]
  );

  const disconnect = useCallback(() => {
    sessionIdRef.current = null;
    cleanup();
    setConnectionStatus('disconnected');
    setSessionId(null);
  }, [cleanup, setConnectionStatus, setSessionId]);

  const sendMessage = useCallback(
    (type: string, payload?: Record<string, unknown>) => {
      if (wsRef.current?.readyState === WebSocket.OPEN) {
        wsRef.current.send(
          JSON.stringify({
            type,
            session_id: sessionIdRef.current,
            payload,
          })
        );
      }
    },
    []
  );

  // Cleanup on unmount
  useEffect(() => {
    return cleanup;
  }, [cleanup]);

  return {
    connect,
    disconnect,
    sendMessage,
    isConnected: useFlowStore((s) => s.connectionStatus) === 'connected',
  };
}
