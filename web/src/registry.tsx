// Component registry + dispatcher (design doc §5.1).
//
// Two layers: `coreRegistry` (12 generic components, used by any document type)
// and `domainRegistry` (per-domain primitives). A node whose `kind` is not in
// the merged registry is not rendered — the client-side half of the security
// boundary (the Rust validator already rejected it; this is defence in depth).

import type { UiNode, UiNodeKind } from "./ir";
import type { OnJump } from "./components";
import * as C from "./components";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type NodeComponent = (props: { node: any; onJump?: OnJump }) => preact.ComponentChild;

const coreRegistry: Partial<Record<UiNodeKind, NodeComponent>> = {
  Tabs: C.Tabs,
  Timeline: C.Timeline,
  Checklist: C.Checklist,
  DataTable: C.DataTable,
  Diagram: C.Diagram,
  Callout: C.Callout,
  RiskPanel: C.RiskPanel,
  ApiExplorer: C.ApiExplorer,
  ConfigViewer: C.ConfigViewer,
  DependencyGraph: C.DependencyGraph,
  LogTimeline: C.LogTimeline,
  CommitGraph: C.CommitGraph,
};

const domainRegistry: Partial<Record<UiNodeKind, NodeComponent>> = {
  Glossary: C.Glossary,
  CharacterRoster: C.CharacterRoster,
  StepNavigator: C.StepNavigator,
  ToleranceMeter: C.ToleranceMeter,
  ScalableTable: C.ScalableTable,
  ObligationMatrix: C.ObligationMatrix,
};

export const registry: Partial<Record<UiNodeKind, NodeComponent>> = {
  ...coreRegistry,
  ...domainRegistry,
};

/** Render a single UI IR node via the registry. Unknown kinds render nothing. */
export function Render({ node, onJump }: { node: UiNode; onJump?: OnJump }) {
  const Component = registry[node.kind];
  if (!Component) {
    // Should be unreachable (validator rejects unknown kinds), but never fail.
    return null;
  }
  return <Component node={node} onJump={onJump} />;
}

/** Render an ordered list of nodes (the Generated UI pane content). */
export function RenderList({ nodes, onJump }: { nodes: UiNode[]; onJump?: OnJump }) {
  return (
    <>
      {nodes.map((n, i) => (
        <Render key={i} node={n} onJump={onJump} />
      ))}
    </>
  );
}
