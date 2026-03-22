# Cascading Hierarchical Agentic Spec-driven Engineering

CHASE is a methodology for rapidly developing multi-component systems with humans and AI agents. It is designed for exploratory work, where breaking changes are often the fastest way to improve architecture, but become harder once many components depend on one another.

CHASE addresses this by requiring explicit component boundaries, direct dependencies, component-level specs, and cascading updates through the dependency DAG. It accounts for the slower information processing of humans by keeping them focused on reviewing load-bearing abstractions rather than every line of code, which can be offloaded to AI.

## Building blocks

### Component

A component is a unit of work created by an AI agent from a spec. It should be small enough for a human to become familiar with it within an hour, and small enough to fit into 10% of the context size of a large model.

### Spec

A spec should focus on intent. It should include a high-level description of the interface, the contract, and the key design decisions, while leaving out implementation details that can be inferred. When developing a spec, one should keep an important trade-off in mind: if the spec is not detailed enough, the implementation and later updates may drift away from the developer's original intent; if it is too detailed, the human may spend too much time working on it.

### Dependency

Each spec must explicitly list its direct dependencies. Dependencies are the components explicitly stated as dependencies in the spec.

The dependency graph must be acyclic. In other words, cyclic dependencies are not allowed.

## Working with CHASE

### Cascading update process

The human drives cascading updates. The agent may help identify components whose dependencies have changed and may require review.

Updates proceed in topological order through reverse dependencies. Each component whose dependencies have changed should be reviewed. If its spec or implementation changes, that change may trigger review of its dependents; if it does not change, no further update is triggered through that component.

If a spec and implementation disagree, the human should be involved. Diffs of the dependencies and their implementations should be provided. An agent and human should jointly update the component spec if needed. Afterwards, the agent should update the implementation as agreed.

### Retroactively converting projects to CHASE

One may want to convert an existing project to CHASE, e.g., to quickly evolve its core parts. In that case, one should first split the project into components and then write spec files based on the implementation. The specs should match the implementation, so that it would be plausible to obtain that implementation from the spec.
