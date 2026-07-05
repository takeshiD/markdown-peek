// Layer 3.5 の domainRegistry (AGENTS.md §5.1 2 層 registry の外側層)。
//
// 統合時: Layer 3 の web/src/registry.ts に
//   const registry = { ...coreRegistry, ...domainRegistry };
// として合流させる (README「統合手順」)。kind は Rust の DomainNode::KINDS /
// DOMAIN_KINDS と 1:1 で一致させること (未知 kind は描画しない = §5.1)。
import type { FunctionComponent } from "preact";
import type { DomainNode } from "./ir";

import { Glossary } from "./components/Glossary";
import { CharacterRoster } from "./components/CharacterRoster";
import { StepNavigator } from "./components/StepNavigator";
import { ToleranceMeter } from "./components/ToleranceMeter";
import { ScalableTable } from "./components/ScalableTable";
import { ObligationMatrix } from "./components/ObligationMatrix";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const domainRegistry: Record<DomainNode["kind"], FunctionComponent<{ node: any }>> = {
  Glossary,
  CharacterRoster,
  StepNavigator,
  ToleranceMeter,
  ScalableTable,
  ObligationMatrix,
};

// Rust 側 DOMAIN_KINDS と突合するための allowlist。
export const DOMAIN_KINDS = Object.keys(domainRegistry) as DomainNode["kind"][];
