use std::collections::BTreeSet;

use petgraph::stable_graph::{NodeIndex, StableDiGraph};

/// Returns a directed cycle, repeating the first node at the end.
pub(crate) fn find_cycle<N, E>(graph: &StableDiGraph<N, E>) -> Option<Vec<NodeIndex>> {
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut stack = Vec::new();

    for index in graph.node_indices() {
        if visited.contains(&index) {
            continue;
        }

        if let Some(cycle) = find_cycle_from(graph, index, &mut visiting, &mut visited, &mut stack)
        {
            return Some(cycle);
        }
    }

    None
}

fn find_cycle_from<N, E>(
    graph: &StableDiGraph<N, E>,
    index: NodeIndex,
    visiting: &mut BTreeSet<NodeIndex>,
    visited: &mut BTreeSet<NodeIndex>,
    stack: &mut Vec<NodeIndex>,
) -> Option<Vec<NodeIndex>> {
    visiting.insert(index);
    stack.push(index);

    for neighbor in graph.neighbors(index) {
        if visiting.contains(&neighbor) {
            let start = stack
                .iter()
                .position(|stack_index| *stack_index == neighbor)?;
            let mut cycle = stack[start..].to_vec();
            cycle.push(neighbor);
            return Some(cycle);
        }

        if !visited.contains(&neighbor) {
            if let Some(cycle) = find_cycle_from(graph, neighbor, visiting, visited, stack) {
                return Some(cycle);
            }
        }
    }

    stack.pop();
    visiting.remove(&index);
    visited.insert(index);

    None
}
