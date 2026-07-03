# Known divergences (not yet byte-exact)

Repro inputs where sebastian diverges from mermaid-cli 11.15.0. Not wired into
corpus tests (they would fail). Fix, then promote to the matching `*_cases/` dir.

- `er_relationship_curve.mmd` — erDiagram relationship edges: our dagre edge
  point-list yields even-thirds cubic control points; mermaid routes through
  extra waypoints so the `curveBasis` control points differ (edge path 11+).
  Curve math itself is correct (flowchart is byte-exact); the bug is in the er
  edge waypoint generation / dagre routing.
- `gitgraph_merge_viewbox.mmd` — gitGraph with a branch+merge: viewBox height
  differs by ~1.30px (172.9455 vs our 174.2459). Content identical; a getBBox
  height accumulation difference (likely commit/branch label measurement).
