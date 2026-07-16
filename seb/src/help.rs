//! `seb --help [topic]`: usage plus format documentation for the
//! sebastian-only extension diagram types (`system_chart`, `pyramid`).

/// General help: usage, options, and the list of help topics.
pub const GENERAL: &str = "\
usage: seb -i input.mmd [-o output.svg] [--id svg-id]
       seb --logo
       seb --help [topic]

options:
  -i, --input <file>   input .mmd diagram
  -o, --output <file>  output SVG path (default: print to stdout)
  --id <id>            id attribute of the generated <svg> (default: my-svg)
  --logo               print the sebastian logo and exit
  -h, --help [topic]   this help, or the format docs for a topic

help topics — sebastian extension formats (no mermaid equivalent):
  system_chart   system architecture: icon boxes connected by labelled arrows
  pyramid        pyramid chart / pyramid of components

Standard mermaid diagram types (flowchart, sequence, gantt, …) follow the
upstream mermaid syntax: https://mermaid.js.org
";

const SYSTEM_CHART: &str = "\
system_chart — sebastian extension

Boxes with typical system-component icons (queue, db, wiki, user, router,
llm, …) connected by labelled arrows, expressing a system architecture.

  system_chart
    title Query pipeline
    query: chat \"AI Agent Query\" \"What is our churn rate?\"
    rt: router \"Router\" \"(Classify)\"
    rag: (db) \"RAG\" \"(Vector DB)\"
    query --> rt
    rt --> rag : Exploratory?

Leading whitespace is ignored; lines starting with %% or # are comments.

  title <text>   optional centred heading
  legend         optional; draws a key of the connection types used

Nodes (one per line, declared before any edge that uses them):
  id: symbol \"Title\" [\"Subtitle\"]
  id: (symbol) \"Title\"    parentheses drop the enclosing box: the node
                          renders as a larger, more prominent icon with
                          the text centred underneath

Edges (label optional; edges take the accent colour of their source node):
  a --> b [: label]   synchronous call / request     solid arrow
  a ..> b [: label]   event trigger / async          dashed arrow
  a ==> b [: label]   message via queue or bus       thick, envelope at midpoint
  a --- b [: label]   undirected association         thin line, no arrowhead

Symbols (each with its own accent colour and icon):
  user users chat queue folder db wiki router llm doc cloud service lock
  server cache api fn stream scheduler browser mobile metrics mail bucket
  key robot search file files box
box is also the fallback for unknown symbol names.

Hand-drawn look: prefix the chart with %%{init: {\"look\": \"handDrawn\"}}%%
";

const PYRAMID: &str = "\
pyramid — sebastian extension

Stacked trapezoid bands forming a triangle (narrow apex on top, wide base
at the bottom), one labelled band per level. Adding a component list turns
a band into a row of component boxes; the two forms mix freely.

  pyramid
    title Architecture
    Presentation: Web, Mobile
    Business: Auth, Orders, Billing
    Data: Postgres, Redis

Leading whitespace is ignored; lines starting with %% or # are comments.

  title <text>       optional centred heading
  <Label>            plain band (pyramid chart)
  <Label>: a, b, c   band with named component boxes laid out in a row

Hand-drawn look: prefix the chart with %%{init: {\"look\": \"handDrawn\"}}%%
";

/// Prints the help for `topic` (general help when `None`). Returns `false`
/// for an unknown topic, after listing the valid ones on stderr.
pub fn print(topic: Option<&str>) -> bool {
    match topic {
        None => print!("{GENERAL}"),
        Some("system_chart") => print!("{SYSTEM_CHART}"),
        Some("pyramid") => print!("{PYRAMID}"),
        Some(other) => {
            eprintln!("unknown help topic: {other} (topics: system_chart, pyramid)");
            return false;
        }
    }
    true
}
