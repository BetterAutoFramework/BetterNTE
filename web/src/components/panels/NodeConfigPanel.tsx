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
  transitions?: TransitionDef[];
}

interface TransitionDef {
  target: string;
  condition: { type: string; [key: string]: unknown };
  priority?: number;
  interrupt?: boolean;
}

export function NodeConfigPanel() {
  const selectedNode = useFlowStore((s) => s.selectedNode);
  const updateNode = useFlowStore((s) => s.updateNode);
  const nodes = useFlowStore((s) => s.nodes);

  if (!selectedNode || selectedNode.type === 'start') {
    return (
      <div style={{ padding: '16px', color: '#9ca3af', textAlign: 'center' }}>
        <p style={{ fontSize: '14px' }}>Select a step node to configure</p>
      </div>
    );
  }

  const data = selectedNode.data as unknown as StepNodeData;
  const kind = data.kind || { type: 'none' };

  const handleKindChange = (field: string, value: unknown) => {
    updateNode(selectedNode.id, {
      kind: { ...kind, [field]: value },
    });
  };

  const handleCommonChange = (field: string, value: unknown) => {
    updateNode(selectedNode.id, { [field]: value });
  };

  const renderKindFields = () => {
    switch (kind.type) {
      case 'script':
        return (
          <Field label="Script Name">
            <input
              className="config-input"
              value={(kind.script as string) || ''}
              onChange={(e) => handleKindChange('script', e.target.value)}
              placeholder="script name"
            />
          </Field>
        );
      case 'click':
        return (
          <>
            <Field label="X">
              <input
                className="config-input"
                type="number"
                value={kind.x ?? 0}
                onChange={(e) => handleKindChange('x', parseInt(e.target.value) || 0)}
              />
            </Field>
            <Field label="Y">
              <input
                className="config-input"
                type="number"
                value={kind.y ?? 0}
                onChange={(e) => handleKindChange('y', parseInt(e.target.value) || 0)}
              />
            </Field>
          </>
        );
      case 'swipe':
        return (
          <>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '8px' }}>
              <Field label="X1">
                <input
                  className="config-input"
                  type="number"
                  value={kind.x1 ?? 0}
                  onChange={(e) => handleKindChange('x1', parseInt(e.target.value) || 0)}
                />
              </Field>
              <Field label="Y1">
                <input
                  className="config-input"
                  type="number"
                  value={kind.y1 ?? 0}
                  onChange={(e) => handleKindChange('y1', parseInt(e.target.value) || 0)}
                />
              </Field>
              <Field label="X2">
                <input
                  className="config-input"
                  type="number"
                  value={kind.x2 ?? 0}
                  onChange={(e) => handleKindChange('x2', parseInt(e.target.value) || 0)}
                />
              </Field>
              <Field label="Y2">
                <input
                  className="config-input"
                  type="number"
                  value={kind.y2 ?? 0}
                  onChange={(e) => handleKindChange('y2', parseInt(e.target.value) || 0)}
                />
              </Field>
            </div>
            <Field label="Duration (ms)">
              <input
                className="config-input"
                type="number"
                value={kind.duration_ms ?? 300}
                onChange={(e) => handleKindChange('duration_ms', parseInt(e.target.value) || 300)}
              />
            </Field>
          </>
        );
      case 'key_press':
        return (
          <Field label="Key">
            <input
              className="config-input"
              value={(kind.key as string) || ''}
              onChange={(e) => handleKindChange('key', e.target.value)}
              placeholder="e.g. enter, escape"
            />
          </Field>
        );
      case 'wait':
        return (
          <Field label="Duration (ms)">
            <input
              className="config-input"
              type="number"
              value={kind.ms ?? 1000}
              onChange={(e) => handleKindChange('ms', parseInt(e.target.value) || 1000)}
            />
          </Field>
        );
      case 'set_variable':
        return (
          <>
            <Field label="Variable">
              <input
                className="config-input"
                value={(kind.key as string) || ''}
                onChange={(e) => handleKindChange('key', e.target.value)}
                placeholder="variable name"
              />
            </Field>
            <Field label="Value">
              <input
                className="config-input"
                value={String(kind.value ?? '')}
                onChange={(e) => {
                  let val: unknown = e.target.value;
                  try { val = JSON.parse(e.target.value); } catch { /* keep as string */ }
                  handleKindChange('value', val);
                }}
                placeholder="value or expression"
              />
            </Field>
          </>
        );
      case 'flow':
        return (
          <Field label="Flow ID">
            <input
              className="config-input"
              value={(kind.flow as string) || ''}
              onChange={(e) => handleKindChange('flow', e.target.value)}
              placeholder="sub-flow id"
            />
          </Field>
        );
      case 'group':
        return (
          <Field label="Group ID">
            <input
              className="config-input"
              value={(kind.group as string) || ''}
              onChange={(e) => handleKindChange('group', e.target.value)}
              placeholder="group id"
            />
          </Field>
        );
      default:
        return <p style={{ color: '#6b7280', fontSize: '12px' }}>No type-specific fields</p>;
    }
  };

  // Available step targets for on_error
  const stepNodes = nodes.filter((n) => n.type === 'step' && n.id !== selectedNode.id);

  return (
    <div
      style={{
        padding: '12px',
        overflowY: 'auto',
        height: '100%',
      }}
    >
      <style>{`
        .config-input {
          width: 100%;
          padding: 4px 8px;
          background: #1a1a2e;
          border: 1px solid #374151;
          border-radius: 4px;
          color: #e0e0e0;
          font-size: 12px;
          outline: none;
          box-sizing: border-box;
        }
        .config-input:focus {
          border-color: #3b82f6;
        }
        .config-select {
          width: 100%;
          padding: 4px 8px;
          background: #1a1a2e;
          border: 1px solid #374151;
          border-radius: 4px;
          color: #e0e0e0;
          font-size: 12px;
          outline: none;
        }
      `}</style>

      <h3 style={{ margin: '0 0 12px', color: '#e0e0e0', fontSize: '14px' }}>
        Step Configuration
      </h3>

      <Field label="Label">
        <input
          className="config-input"
          value={data.label || ''}
          onChange={(e) => handleCommonChange('label', e.target.value)}
        />
      </Field>

      <Field label="Type">
        <select
          className="config-select"
          value={kind.type}
          onChange={(e) =>
            updateNode(selectedNode.id, {
              kind: { type: e.target.value },
            })
          }
        >
          <option value="none">None</option>
          <option value="script">Script</option>
          <option value="click">Click</option>
          <option value="swipe">Swipe</option>
          <option value="key_press">Key Press</option>
          <option value="wait">Wait</option>
          <option value="set_variable">Set Variable</option>
          <option value="flow">Flow</option>
          <option value="group">Group</option>
        </select>
      </Field>

      <div style={{ borderTop: '1px solid #374151', margin: '12px 0', paddingTop: '12px' }}>
        <h4 style={{ margin: '0 0 8px', color: '#9ca3af', fontSize: '12px' }}>
          Type Settings
        </h4>
        {renderKindFields()}
      </div>

      <div style={{ borderTop: '1px solid #374151', margin: '12px 0', paddingTop: '12px' }}>
        <h4 style={{ margin: '0 0 8px', color: '#9ca3af', fontSize: '12px' }}>
          Common Settings
        </h4>
        <Field label="Timeout (ms)">
          <input
            className="config-input"
            type="number"
            value={data.timeout_ms ?? ''}
            onChange={(e) =>
              handleCommonChange(
                'timeout_ms',
                e.target.value ? parseInt(e.target.value) : undefined
              )
            }
            placeholder="optional"
          />
        </Field>
        <Field label="Max Retries">
          <input
            className="config-input"
            type="number"
            value={data.max_retries ?? 0}
            onChange={(e) =>
              handleCommonChange('max_retries', parseInt(e.target.value) || 0)
            }
          />
        </Field>
        <Field label="On Error →">
          <select
            className="config-select"
            value={data.on_error || ''}
            onChange={(e) =>
              handleCommonChange('on_error', e.target.value || undefined)
            }
          >
            <option value="">None</option>
            {stepNodes.map((n) => {
              const d = n.data as unknown as StepNodeData;
              return (
                <option key={n.id} value={d.label || n.id}>
                  {d.label || n.id}
                </option>
              );
            })}
          </select>
        </Field>
      </div>

      <div style={{ borderTop: '1px solid #374151', margin: '12px 0', paddingTop: '12px' }}>
        <h4 style={{ margin: '0 0 8px', color: '#9ca3af', fontSize: '12px' }}>
          Transitions
        </h4>
        {(data.transitions || []).map((t, i) => (
          <div
            key={i}
            style={{
              background: '#1a1a2e',
              border: '1px solid #374151',
              borderRadius: '4px',
              padding: '6px 8px',
              marginBottom: '6px',
              fontSize: '11px',
            }}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between' }}>
              <span style={{ color: '#e0e0e0' }}>→ {t.target}</span>
              <button
                onClick={() => {
                  const transitions = [...(data.transitions || [])];
                  transitions.splice(i, 1);
                  handleCommonChange('transitions', transitions);
                }}
                style={{
                  background: 'none',
                  border: 'none',
                  color: '#ef4444',
                  cursor: 'pointer',
                  fontSize: '11px',
                  padding: '0 4px',
                }}
              >
                ✕
              </button>
            </div>
            <span style={{ color: '#6b7280' }}>
              {t.condition?.type || 'always'} • priority: {t.priority ?? 0}
            </span>
          </div>
        ))}
        <button
          onClick={() => {
            const transitions = [...(data.transitions || [])];
            transitions.push({
              target: '',
              condition: { type: 'always' },
              priority: 0,
              interrupt: false,
            });
            handleCommonChange('transitions', transitions);
          }}
          style={{
            width: '100%',
            padding: '4px',
            background: '#0f3460',
            border: '1px solid #374151',
            borderRadius: '4px',
            color: '#e0e0e0',
            cursor: 'pointer',
            fontSize: '12px',
          }}
        >
          + Add Transition
        </button>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: '8px' }}>
      <label
        style={{
          display: 'block',
          fontSize: '11px',
          color: '#9ca3af',
          marginBottom: '3px',
          textTransform: 'uppercase',
          letterSpacing: '0.5px',
        }}
      >
        {label}
      </label>
      {children}
    </div>
  );
}
