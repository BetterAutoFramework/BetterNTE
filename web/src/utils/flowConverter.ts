import type { Node, Edge } from '@xyflow/react';

// StepKind type matching Rust StepKind
interface StepKind {
  type: string;
  [key: string]: unknown;
}

interface StepData {
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

interface FlowStep {
  kind: StepKind;
  input?: Record<string, string>;
  output?: Record<string, string>;
  timeout_ms?: number | null;
  max_retries?: number;
  on_error?: string | null;
  transitions: {
    target: string;
    condition: { type: string; [key: string]: unknown };
    priority: number;
    interrupt: boolean;
  }[];
}

interface FlowJSON {
  id: string;
  name: string;
  description: string;
  version: string;
  entry: string;
  steps: Record<string, FlowStep>;
  variables: Record<string, unknown>;
  tags: string[];
}

export function toFlowJSON(
  nodes: Node[],
  edges: Edge[],
  flowName: string
): Record<string, unknown> {
  const steps: Record<string, FlowStep> = {};
  let entryId = '';

  // Build step map
  for (const node of nodes) {
    const data = node.data as StepData;
    if (node.type === 'start') {
      entryId = `start_${node.id}`;
      continue;
    }

    const stepId = data.label || node.id;
    const kind = data.kind || { type: 'none' };

    // Build transitions from edges
    const transitions: FlowStep['transitions'] = [];
    const outEdges = edges.filter((e) => e.source === node.id);
    for (const edge of outEdges) {
      const targetNode = nodes.find((n) => n.id === edge.target);
      if (!targetNode) continue;
      const targetData = targetNode.data as StepData;
      const targetId = targetData.label || targetNode.id;

      // Use edge data for condition if available
      const edgeData = edge.data as { condition?: TransitionDef['condition']; priority?: number } | undefined;
      transitions.push({
        target: targetId,
        condition: edgeData?.condition || { type: 'always' },
        priority: edgeData?.priority ?? 0,
        interrupt: false,
      });
    }

    steps[stepId] = {
      kind,
      input: data.input || {},
      output: data.output || {},
      timeout_ms: data.timeout_ms ?? null,
      max_retries: data.max_retries ?? 0,
      on_error: data.on_error ?? null,
      transitions,
    };
  }

  // Determine entry step
  if (!entryId && nodes.length > 0) {
    // Find start node's target
    const startNode = nodes.find((n) => n.type === 'start');
    if (startNode) {
      const startEdge = edges.find((e) => e.source === startNode.id);
      if (startEdge) {
        const targetNode = nodes.find((n) => n.id === startEdge.target);
        if (targetNode) {
          const targetData = targetNode.data as StepData;
          entryId = targetData.label || targetNode.id;
        }
      }
    }
    if (!entryId) {
      // Fallback to first non-start node
      const firstStep = nodes.find((n) => n.type !== 'start');
      if (firstStep) {
        const d = firstStep.data as StepData;
        entryId = d.label || firstStep.id;
      }
    }
  }

  const flow: FlowJSON = {
    id: `flow_${Date.now()}`,
    name: flowName,
    description: '',
    version: '1.0.0',
    entry: entryId,
    steps,
    variables: {},
    tags: [],
  };

  return flow as unknown as Record<string, unknown>;
}

export function fromFlowJSON(json: Record<string, unknown>): {
  nodes: Node[];
  edges: Edge[];
} {
  const flow = json as unknown as FlowJSON;
  const nodes: Node[] = [];
  const edges: Edge[] = [];

  // Create start node
  const startNode: Node = {
    id: 'start',
    type: 'start',
    position: { x: 50, y: 200 },
    data: { label: 'Start' },
  };
  nodes.push(startNode);

  const stepEntries = Object.entries(flow.steps || {});
  const stepPositions: Record<string, { x: number; y: number }> = {};

  // Layout: arrange in a grid
  const cols = Math.ceil(Math.sqrt(stepEntries.length));
  stepEntries.forEach(([stepId], i) => {
    const col = i % cols;
    const row = Math.floor(i / cols);
    stepPositions[stepId] = { x: 250 + col * 300, y: 50 + row * 200 };
  });

  // Create nodes from steps
  for (const [stepId, step] of stepEntries) {
    const node: Node = {
      id: stepId,
      type: 'step',
      position: stepPositions[stepId] || { x: 250, y: 200 },
      data: {
        label: stepId,
        kind: step.kind,
        input: step.input || {},
        output: step.output || {},
        timeout_ms: step.timeout_ms,
        max_retries: step.max_retries,
        on_error: step.on_error,
        transitions: step.transitions || [],
      },
    };
    nodes.push(node);
  }

  // Connect start node to entry step
  if (flow.entry && stepPositions[flow.entry]) {
    edges.push({
      id: `start->${flow.entry}`,
      source: 'start',
      target: flow.entry,
      type: 'default',
    });
  }

  // Create edges from transitions
  for (const [stepId, step] of stepEntries) {
    for (const transition of step.transitions || []) {
      if (stepPositions[transition.target]) {
        edges.push({
          id: `${stepId}->${transition.target}`,
          source: stepId,
          target: transition.target,
          type: 'default',
          data: {
            condition: transition.condition,
            priority: transition.priority,
          },
        });
      }
    }
  }

  return { nodes, edges };
}
