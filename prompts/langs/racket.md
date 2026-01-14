RACKET LANGUAGE CODE EXPLORATION + RACO TOOLING

- Core `raco` commands to rely on:
  - Documentation & discovery:
    - `raco docs <id>` to open docs for identifiers, modules, or packages.
  - Compilation & checks:
    - `raco make <file.rkt>` to force compilation and surface errors early.
  - Testing:
    - `raco test <path>` to run `module+ test` blocks and test files.
  - Packages & dependencies:
    - `raco pkg show` to inspect installed packages and their locations.
    - `raco pkg show <pkg>` to inspect package metadata and versions.
  - Profiling & performance:
    - `raco profile <file.rkt>` for CPU hot spots.
  - Debugging & stack traces:
    - `racket -l errortrace <file.rkt>` (or enabling errortrace) for readable stack traces.

- Structural analysis tools (use when reasoning about non-trivial codebases):
  - `raco dependency-graph <path>`:
    - Use to visualize or reason about module dependencies and layering.
    - Identify cycles, high fan-in “core” modules, and accidental coupling.
  - `raco modgraph`:
    - Use for quick textual inspection of module graphs when visualization isn’t needed.
  - Treat dependency graphs as architectural signals, not just diagrams.

- `racket -e`-driven exploration:
  - Use the one-shot script execution to:
    - `require` modules incrementally and inspect exports.
    - Probe functions with small concrete examples.
    - Validate assumptions about data shapes and return values.
