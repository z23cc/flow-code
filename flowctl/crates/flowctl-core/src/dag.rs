//! petgraph-based DAG for task dependency resolution.
//!
//! Uses `StableDiGraph` for stable node indices across runtime mutations
//! (split_task, skip_task). Implements Kahn's algorithm manually for
//! topological sorting — petgraph's `Topo` iterator is ~10x slower
//! (see petgraph#665).

use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::graph::NodeIndex;
use petgraph::stable_graph::StableDiGraph;
use petgraph::Direction;

use crate::error::CoreError;
use crate::state_machine::Status;
use crate::types::Task;

/// A directed acyclic graph of task dependencies.
///
/// Edges point from dependency to dependent: if B depends on A,
/// the edge is A -> B. This means "A must complete before B".
#[derive(Debug)]
pub struct TaskDag {
    /// The underlying petgraph graph. Node weight is the task ID string.
    graph: StableDiGraph<String, ()>,
    /// O(1) lookup from task ID to graph node index.
    index: HashMap<String, NodeIndex>,
}

impl TaskDag {
    /// Build a DAG from a slice of tasks.
    ///
    /// Cross-epic dependencies are supported: a task's `depends_on` may
    /// reference task IDs from other epics as long as they appear in the
    /// input slice.
    pub fn from_tasks(tasks: &[Task]) -> Result<Self, CoreError> {
        let mut graph = StableDiGraph::new();
        let mut index = HashMap::with_capacity(tasks.len());

        // Phase 1: add all nodes.
        for task in tasks {
            if index.contains_key(&task.id) {
                return Err(CoreError::DuplicateTask(task.id.clone()));
            }
            let ni = graph.add_node(task.id.clone());
            index.insert(task.id.clone(), ni);
        }

        // Phase 2: add edges (dependency -> dependent).
        for task in tasks {
            let dependent_ni = index[&task.id];
            for dep_id in &task.depends_on {
                let dep_ni = *index.get(dep_id.as_str()).ok_or_else(|| {
                    CoreError::UnknownDependency {
                        task: task.id.clone(),
                        dependency: dep_id.clone(),
                    }
                })?;
                // Self-reference check.
                if dep_ni == dependent_ni {
                    return Err(CoreError::CycleDetected(format!(
                        "self-referencing: {} -> {}",
                        task.id, dep_id
                    )));
                }
                graph.add_edge(dep_ni, dependent_ni, ());
            }
        }

        Ok(TaskDag { graph, index })
    }

    /// Return task IDs whose dependencies are all satisfied.
    ///
    /// A task is "ready" when:
    /// - Its own status is `Todo` (not started, not blocked, not done)
    /// - All of its dependencies have a satisfied status (`Done` or `Skipped`)
    pub fn ready_tasks(&self, statuses: &HashMap<String, Status>) -> Vec<String> {
        let mut ready = Vec::new();
        for (id, &ni) in &self.index {
            let status = statuses.get(id.as_str()).copied().unwrap_or(Status::Todo);
            if status != Status::Todo {
                continue;
            }
            let all_deps_satisfied = self
                .graph
                .neighbors_directed(ni, Direction::Incoming)
                .all(|dep_ni| {
                    let dep_id = &self.graph[dep_ni];
                    statuses
                        .get(dep_id.as_str())
                        .copied()
                        .unwrap_or(Status::Todo)
                        .is_satisfied()
                });
            if all_deps_satisfied {
                ready.push(id.clone());
            }
        }
        ready.sort(); // deterministic ordering
        ready
    }

    /// Mark a task as complete and return newly-ready downstream task IDs.
    ///
    /// "Newly ready" means: status is `Todo` and all deps are now satisfied.
    /// The caller is responsible for actually updating the status map.
    pub fn complete(&self, id: &str, statuses: &HashMap<String, Status>) -> Vec<String> {
        let Some(&ni) = self.index.get(id) else {
            return vec![];
        };
        let mut newly_ready = Vec::new();
        // Check each downstream dependent.
        for dep_ni in self.graph.neighbors_directed(ni, Direction::Outgoing) {
            let dep_id = &self.graph[dep_ni];
            let dep_status = statuses
                .get(dep_id.as_str())
                .copied()
                .unwrap_or(Status::Todo);
            if dep_status != Status::Todo {
                continue;
            }
            // Check if ALL of this dependent's deps are satisfied
            // (treating the just-completed task as satisfied).
            let all_satisfied = self
                .graph
                .neighbors_directed(dep_ni, Direction::Incoming)
                .all(|upstream_ni| {
                    let upstream_id = &self.graph[upstream_ni];
                    if upstream_id == id {
                        return true; // the one we just completed
                    }
                    statuses
                        .get(upstream_id.as_str())
                        .copied()
                        .unwrap_or(Status::Todo)
                        .is_satisfied()
                });
            if all_satisfied {
                newly_ready.push(dep_id.clone());
            }
        }
        newly_ready.sort();
        newly_ready
    }

    /// Propagate failure: return all transitively downstream task IDs.
    pub fn propagate_failure(&self, id: &str) -> Vec<String> {
        let Some(&ni) = self.index.get(id) else {
            return vec![];
        };
        let mut affected = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(ni);
        visited.insert(ni);
        while let Some(current) = queue.pop_front() {
            for downstream in self.graph.neighbors_directed(current, Direction::Outgoing) {
                if visited.insert(downstream) {
                    affected.push(self.graph[downstream].clone());
                    queue.push_back(downstream);
                }
            }
        }
        affected.sort();
        affected
    }

    /// Detect cycles using Kahn's algorithm. Returns `None` if the graph is
    /// a valid DAG, or `Some(cycle_members)` listing task IDs involved.
    pub fn detect_cycles(&self) -> Option<Vec<String>> {
        let node_count = self.graph.node_count();
        if node_count == 0 {
            return None;
        }

        // Kahn's algorithm: compute in-degrees, process zero-degree nodes.
        let mut in_degree: HashMap<NodeIndex, usize> = HashMap::with_capacity(node_count);
        for ni in self.graph.node_indices() {
            in_degree.insert(
                ni,
                self.graph
                    .neighbors_directed(ni, Direction::Incoming)
                    .count(),
            );
        }

        let mut queue: VecDeque<NodeIndex> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(ni, _)| *ni)
            .collect();

        let mut processed = 0usize;
        while let Some(ni) = queue.pop_front() {
            processed += 1;
            for downstream in self.graph.neighbors_directed(ni, Direction::Outgoing) {
                if let Some(deg) = in_degree.get_mut(&downstream) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(downstream);
                    }
                }
            }
        }

        if processed == node_count {
            None
        } else {
            // Nodes with remaining in-degree > 0 are in cycles.
            let mut cycle_members: Vec<String> = in_degree
                .iter()
                .filter(|(_, deg)| **deg > 0)
                .map(|(ni, _)| self.graph[*ni].clone())
                .collect();
            cycle_members.sort();
            Some(cycle_members)
        }
    }

    /// Compute the critical path (longest path through the DAG).
    ///
    /// Each task has unit weight (1). Returns task IDs on the longest path
    /// from any source to any sink, in topological order.
    pub fn critical_path(&self) -> Vec<String> {
        if self.graph.node_count() == 0 {
            return vec![];
        }

        // Topological order via Kahn's.
        let topo_order = self.topological_sort();

        // Longest-path DP.
        let mut dist: HashMap<NodeIndex, usize> = HashMap::new();
        let mut pred: HashMap<NodeIndex, NodeIndex> = HashMap::new();

        for &ni in &topo_order {
            dist.insert(ni, 0);
        }

        for &ni in &topo_order {
            let current_dist = dist[&ni];
            for downstream in self.graph.neighbors_directed(ni, Direction::Outgoing) {
                let new_dist = current_dist + 1;
                if new_dist > *dist.get(&downstream).unwrap_or(&0) {
                    dist.insert(downstream, new_dist);
                    pred.insert(downstream, ni);
                }
            }
        }

        // Find the node with the maximum distance (end of critical path).
        let (end_node, _) = dist.iter().max_by_key(|(_, d)| *d).unwrap();

        // Trace back.
        let mut path = vec![*end_node];
        let mut current = *end_node;
        while let Some(p) = pred.get(&current) {
            path.push(*p);
            current = *p;
        }
        path.reverse();

        path.iter().map(|&ni| self.graph[ni].clone()).collect()
    }

    /// Split a task into multiple new tasks, re-wiring dependencies.
    ///
    /// The original task's incoming edges go to the first new task.
    /// The original task's outgoing edges come from the last new task.
    /// New tasks are chained sequentially: new[0] -> new[1] -> ... -> new[n-1].
    pub fn split_task(&mut self, id: &str, new_tasks: Vec<Task>) -> Result<(), CoreError> {
        let &old_ni = self
            .index
            .get(id)
            .ok_or_else(|| CoreError::TaskNotFound(id.to_string()))?;

        if new_tasks.is_empty() {
            return Err(CoreError::InvalidId(
                "split requires at least one replacement task".to_string(),
            ));
        }

        // Check for duplicate IDs among new tasks.
        for t in &new_tasks {
            if self.index.contains_key(&t.id) && t.id != id {
                return Err(CoreError::DuplicateTask(t.id.clone()));
            }
        }

        // Collect incoming/outgoing neighbors before mutation.
        let incoming: Vec<NodeIndex> = self
            .graph
            .neighbors_directed(old_ni, Direction::Incoming)
            .collect();
        let outgoing: Vec<NodeIndex> = self
            .graph
            .neighbors_directed(old_ni, Direction::Outgoing)
            .collect();

        // Remove old node.
        self.graph.remove_node(old_ni);
        self.index.remove(id);

        // Add new nodes.
        let mut new_indices = Vec::with_capacity(new_tasks.len());
        for t in &new_tasks {
            let ni = self.graph.add_node(t.id.clone());
            self.index.insert(t.id.clone(), ni);
            new_indices.push(ni);
        }

        // Wire incoming edges to first new node.
        for inc in &incoming {
            self.graph.add_edge(*inc, new_indices[0], ());
        }

        // Wire last new node to outgoing edges.
        let last = *new_indices.last().unwrap();
        for out in &outgoing {
            self.graph.add_edge(last, *out, ());
        }

        // Chain new tasks sequentially.
        for w in new_indices.windows(2) {
            self.graph.add_edge(w[0], w[1], ());
        }

        Ok(())
    }

    /// Skip a task: treat it as satisfied for dependency resolution.
    /// Returns the list of downstream task IDs that may now be ready.
    ///
    /// The caller should set the task's status to `Skipped` in their status map
    /// and then call `ready_tasks` or use the returned list directly.
    pub fn skip_task(&self, id: &str, statuses: &HashMap<String, Status>) -> Vec<String> {
        // Skipped is treated as satisfied, so this is the same as complete.
        self.complete(id, statuses)
    }

    /// Return all node indices in topological order (Kahn's algorithm).
    pub fn topological_sort(&self) -> Vec<NodeIndex> {
        let node_count = self.graph.node_count();
        let mut in_degree: HashMap<NodeIndex, usize> = HashMap::with_capacity(node_count);
        for ni in self.graph.node_indices() {
            in_degree.insert(
                ni,
                self.graph
                    .neighbors_directed(ni, Direction::Incoming)
                    .count(),
            );
        }

        let mut queue: VecDeque<NodeIndex> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(ni, _)| *ni)
            .collect();

        let mut order = Vec::with_capacity(node_count);
        while let Some(ni) = queue.pop_front() {
            order.push(ni);
            for downstream in self.graph.neighbors_directed(ni, Direction::Outgoing) {
                if let Some(deg) = in_degree.get_mut(&downstream) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(downstream);
                    }
                }
            }
        }
        order
    }

    /// Return task IDs in topological order.
    pub fn topological_sort_ids(&self) -> Vec<String> {
        self.topological_sort()
            .iter()
            .map(|&ni| self.graph[ni].clone())
            .collect()
    }

    /// Number of tasks in the DAG.
    pub fn len(&self) -> usize {
        self.graph.node_count()
    }

    /// Whether the DAG is empty.
    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }

    /// Check if a task ID exists in the DAG.
    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// Get direct dependencies (upstream) for a task.
    pub fn dependencies(&self, id: &str) -> Vec<String> {
        let Some(&ni) = self.index.get(id) else {
            return vec![];
        };
        let mut deps: Vec<String> = self
            .graph
            .neighbors_directed(ni, Direction::Incoming)
            .map(|dep_ni| self.graph[dep_ni].clone())
            .collect();
        deps.sort();
        deps
    }

    /// Return all task IDs in the DAG (sorted for determinism).
    pub fn task_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.index.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Get direct dependents (downstream) for a task.
    pub fn dependents(&self, id: &str) -> Vec<String> {
        let Some(&ni) = self.index.get(id) else {
            return vec![];
        };
        let mut deps: Vec<String> = self
            .graph
            .neighbors_directed(ni, Direction::Outgoing)
            .map(|dep_ni| self.graph[dep_ni].clone())
            .collect();
        deps.sort();
        deps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Domain;
    use chrono::Utc;

    /// Helper to create a minimal task with the given ID and dependencies.
    fn make_task(id: &str, deps: &[&str]) -> Task {
        Task {
            schema_version: 1,
            id: id.to_string(),
            epic: "test-epic".to_string(),
            title: format!("Task {id}"),
            status: Status::Todo,
            priority: None,
            domain: Domain::General,
            depends_on: deps.iter().copied().map(String::from).collect(),
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn status_map(entries: &[(&str, Status)]) -> HashMap<String, Status> {
        entries
            .iter()
            .map(|(id, s)| (id.to_string(), *s))
            .collect()
    }

    // ── Construction ────────────────────────────────────────────────

    #[test]
    fn test_empty_dag() {
        let dag = TaskDag::from_tasks(&[]).unwrap();
        assert!(dag.is_empty());
        assert_eq!(dag.len(), 0);
    }

    #[test]
    fn test_single_task_no_deps() {
        let dag = TaskDag::from_tasks(&[make_task("a", &[])]).unwrap();
        assert_eq!(dag.len(), 1);
        assert!(dag.contains("a"));
    }

    #[test]
    fn test_linear_chain() {
        // a -> b -> c
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["b"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert_eq!(dag.len(), 3);
        assert_eq!(dag.dependencies("b"), vec!["a"]);
        assert_eq!(dag.dependents("a"), vec!["b"]);
        assert_eq!(dag.dependents("b"), vec!["c"]);
    }

    #[test]
    fn test_diamond_dag() {
        //   a
        //  / \
        // b   c
        //  \ /
        //   d
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["a"]),
            make_task("d", &["b", "c"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert_eq!(dag.len(), 4);
        assert_eq!(dag.dependencies("d"), vec!["b", "c"]);
    }

    #[test]
    fn test_duplicate_task_id() {
        let tasks = vec![make_task("a", &[]), make_task("a", &[])];
        let err = TaskDag::from_tasks(&tasks).unwrap_err();
        assert!(matches!(err, CoreError::DuplicateTask(_)));
    }

    #[test]
    fn test_unknown_dependency() {
        let tasks = vec![make_task("a", &["nonexistent"])];
        let err = TaskDag::from_tasks(&tasks).unwrap_err();
        assert!(matches!(err, CoreError::UnknownDependency { .. }));
    }

    #[test]
    fn test_self_reference_detected() {
        let tasks = vec![make_task("a", &["a"])];
        let err = TaskDag::from_tasks(&tasks).unwrap_err();
        assert!(matches!(err, CoreError::CycleDetected(_)));
    }

    #[test]
    fn test_cycle_detected() {
        // a -> b -> c -> a
        let tasks = vec![
            make_task("a", &["c"]),
            make_task("b", &["a"]),
            make_task("c", &["b"]),
        ];
        // from_tasks no longer detects cycles — callers must use detect_cycles()
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert!(dag.detect_cycles().is_some());
    }

    // ── ready_tasks ─────────────────────────────────────────────────

    #[test]
    fn test_ready_tasks_all_todo_no_deps() {
        let tasks = vec![make_task("a", &[]), make_task("b", &[])];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[("a", Status::Todo), ("b", Status::Todo)]);
        let ready = dag.ready_tasks(&statuses);
        assert_eq!(ready, vec!["a", "b"]);
    }

    #[test]
    fn test_ready_tasks_with_deps() {
        // a -> b -> c
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["b"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();

        // Initially only a is ready.
        let statuses = status_map(&[
            ("a", Status::Todo),
            ("b", Status::Todo),
            ("c", Status::Todo),
        ]);
        assert_eq!(dag.ready_tasks(&statuses), vec!["a"]);

        // After a is done, b is ready.
        let statuses = status_map(&[
            ("a", Status::Done),
            ("b", Status::Todo),
            ("c", Status::Todo),
        ]);
        assert_eq!(dag.ready_tasks(&statuses), vec!["b"]);
    }

    #[test]
    fn test_ready_tasks_skipped_satisfies_deps() {
        let tasks = vec![make_task("a", &[]), make_task("b", &["a"])];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[("a", Status::Skipped), ("b", Status::Todo)]);
        assert_eq!(dag.ready_tasks(&statuses), vec!["b"]);
    }

    #[test]
    fn test_ready_tasks_excludes_non_todo() {
        let tasks = vec![make_task("a", &[]), make_task("b", &[])];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[("a", Status::InProgress), ("b", Status::Todo)]);
        assert_eq!(dag.ready_tasks(&statuses), vec!["b"]);
    }

    #[test]
    fn test_ready_tasks_diamond() {
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["a"]),
            make_task("d", &["b", "c"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();

        // a done -> b, c ready.
        let statuses = status_map(&[
            ("a", Status::Done),
            ("b", Status::Todo),
            ("c", Status::Todo),
            ("d", Status::Todo),
        ]);
        assert_eq!(dag.ready_tasks(&statuses), vec!["b", "c"]);

        // Only b done -> d not ready (c still todo).
        let statuses = status_map(&[
            ("a", Status::Done),
            ("b", Status::Done),
            ("c", Status::Todo),
            ("d", Status::Todo),
        ]);
        assert_eq!(dag.ready_tasks(&statuses), vec!["c"]);

        // Both done -> d ready.
        let statuses = status_map(&[
            ("a", Status::Done),
            ("b", Status::Done),
            ("c", Status::Done),
            ("d", Status::Todo),
        ]);
        assert_eq!(dag.ready_tasks(&statuses), vec!["d"]);
    }

    // ── complete ────────────────────────────────────────────────────

    #[test]
    fn test_complete_returns_newly_ready() {
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["a"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[
            ("a", Status::Todo),
            ("b", Status::Todo),
            ("c", Status::Todo),
        ]);
        let newly_ready = dag.complete("a", &statuses);
        assert_eq!(newly_ready, vec!["b", "c"]);
    }

    #[test]
    fn test_complete_diamond_partial() {
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["a"]),
            make_task("d", &["b", "c"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();

        // Complete b with c still todo -> d not ready.
        let statuses = status_map(&[
            ("a", Status::Done),
            ("b", Status::Todo),
            ("c", Status::Todo),
            ("d", Status::Todo),
        ]);
        let newly_ready = dag.complete("b", &statuses);
        assert!(!newly_ready.contains(&"d".to_string()));

        // Complete b with c done -> d ready.
        let statuses = status_map(&[
            ("a", Status::Done),
            ("b", Status::Todo),
            ("c", Status::Done),
            ("d", Status::Todo),
        ]);
        let newly_ready = dag.complete("b", &statuses);
        assert_eq!(newly_ready, vec!["d"]);
    }

    #[test]
    fn test_complete_unknown_task() {
        let dag = TaskDag::from_tasks(&[make_task("a", &[])]).unwrap();
        let statuses = status_map(&[]);
        assert!(dag.complete("nonexistent", &statuses).is_empty());
    }

    // ── propagate_failure ───────────────────────────────────────────

    #[test]
    fn test_propagate_failure_linear() {
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["b"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let affected = dag.propagate_failure("a");
        assert_eq!(affected, vec!["b", "c"]);
    }

    #[test]
    fn test_propagate_failure_diamond() {
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["a"]),
            make_task("d", &["b", "c"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let affected = dag.propagate_failure("a");
        assert_eq!(affected, vec!["b", "c", "d"]);
    }

    #[test]
    fn test_propagate_failure_leaf() {
        let tasks = vec![make_task("a", &[]), make_task("b", &["a"])];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let affected = dag.propagate_failure("b");
        assert!(affected.is_empty());
    }

    #[test]
    fn test_propagate_failure_unknown() {
        let dag = TaskDag::from_tasks(&[make_task("a", &[])]).unwrap();
        assert!(dag.propagate_failure("nonexistent").is_empty());
    }

    // ── detect_cycles ───────────────────────────────────────────────

    #[test]
    fn test_detect_cycles_none() {
        let tasks = vec![make_task("a", &[]), make_task("b", &["a"])];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert!(dag.detect_cycles().is_none());
    }

    #[test]
    fn test_detect_cycles_empty() {
        let dag = TaskDag::from_tasks(&[]).unwrap();
        assert!(dag.detect_cycles().is_none());
    }

    // ── critical_path ───────────────────────────────────────────────

    #[test]
    fn test_critical_path_linear() {
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["b"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let cp = dag.critical_path();
        assert_eq!(cp, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_critical_path_diamond() {
        // a -> b -> d  (length 3)
        // a -> c -> d  (length 3)
        // Both paths are equal length, so either is valid.
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["a"]),
            make_task("d", &["b", "c"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let cp = dag.critical_path();
        assert_eq!(cp.len(), 3);
        assert_eq!(cp[0], "a");
        assert_eq!(cp[2], "d");
    }

    #[test]
    fn test_critical_path_single() {
        let dag = TaskDag::from_tasks(&[make_task("a", &[])]).unwrap();
        let cp = dag.critical_path();
        assert_eq!(cp, vec!["a"]);
    }

    #[test]
    fn test_critical_path_empty() {
        let dag = TaskDag::from_tasks(&[]).unwrap();
        assert!(dag.critical_path().is_empty());
    }

    #[test]
    fn test_critical_path_wide() {
        // a -> b -> c -> d (length 4)
        // a -> e            (length 2)
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["b"]),
            make_task("d", &["c"]),
            make_task("e", &["a"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let cp = dag.critical_path();
        assert_eq!(cp, vec!["a", "b", "c", "d"]);
    }

    // ── split_task ──────────────────────────────────────────────────

    #[test]
    fn test_split_task_basic() {
        // a -> b -> c  =>  a -> b1 -> b2 -> c
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["b"]),
        ];
        let mut dag = TaskDag::from_tasks(&tasks).unwrap();
        dag.split_task("b", vec![make_task("b1", &[]), make_task("b2", &[])])
            .unwrap();

        assert!(!dag.contains("b"));
        assert!(dag.contains("b1"));
        assert!(dag.contains("b2"));
        assert_eq!(dag.len(), 4); // a, b1, b2, c

        assert_eq!(dag.dependencies("b1"), vec!["a"]);
        assert_eq!(dag.dependents("b1"), vec!["b2"]);
        assert_eq!(dag.dependencies("b2"), vec!["b1"]);
        assert_eq!(dag.dependents("b2"), vec!["c"]);
        assert_eq!(dag.dependencies("c"), vec!["b2"]);
    }

    #[test]
    fn test_split_task_single_replacement() {
        let tasks = vec![make_task("a", &[]), make_task("b", &["a"])];
        let mut dag = TaskDag::from_tasks(&tasks).unwrap();
        dag.split_task("b", vec![make_task("b_new", &[])]).unwrap();
        assert!(!dag.contains("b"));
        assert!(dag.contains("b_new"));
        assert_eq!(dag.dependencies("b_new"), vec!["a"]);
    }

    #[test]
    fn test_split_task_not_found() {
        let mut dag = TaskDag::from_tasks(&[make_task("a", &[])]).unwrap();
        let err = dag
            .split_task("nonexistent", vec![make_task("b", &[])])
            .unwrap_err();
        assert!(matches!(err, CoreError::TaskNotFound(_)));
    }

    #[test]
    fn test_split_task_empty_replacements() {
        let mut dag = TaskDag::from_tasks(&[make_task("a", &[])]).unwrap();
        let err = dag.split_task("a", vec![]).unwrap_err();
        assert!(matches!(err, CoreError::InvalidId(_)));
    }

    // ── skip_task ───────────────────────────────────────────────────

    #[test]
    fn test_skip_task_unblocks_downstream() {
        let tasks = vec![make_task("a", &[]), make_task("b", &["a"])];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let statuses = status_map(&[("a", Status::Todo), ("b", Status::Todo)]);
        let newly_ready = dag.skip_task("a", &statuses);
        assert_eq!(newly_ready, vec!["b"]);
    }

    // ── cross-epic dependencies ─────────────────────────────────────

    #[test]
    fn test_cross_epic_deps() {
        let mut task_a = make_task("epic1.1", &[]);
        task_a.epic = "epic1".to_string();
        let mut task_b = make_task("epic2.1", &["epic1.1"]);
        task_b.epic = "epic2".to_string();

        let dag = TaskDag::from_tasks(&[task_a, task_b]).unwrap();
        assert_eq!(dag.dependencies("epic2.1"), vec!["epic1.1"]);
    }

    // ── performance ─────────────────────────────────────────────────

    #[test]
    fn test_1000_task_dag_performance() {
        use std::time::Instant;

        // Build a 1000-task chain: task-0 -> task-1 -> ... -> task-999.
        let tasks: Vec<Task> = (0..1000)
            .map(|i| {
                let deps = if i == 0 {
                    vec![]
                } else {
                    vec![format!("task-{}", i - 1)]
                };
                let mut t = make_task(&format!("task-{i}"), &[]);
                t.depends_on = deps;
                t
            })
            .collect();

        let start = Instant::now();
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(dag.len(), 1000);
        assert!(
            elapsed.as_millis() < 100,
            "1000-task DAG build took {}ms (limit: 100ms)",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_1000_task_wide_dag() {
        // 1 root + 999 tasks all depending on root (star topology).
        let mut tasks = vec![make_task("root", &[])];
        for i in 0..999 {
            tasks.push(make_task(&format!("leaf-{i}"), &["root"]));
        }

        let start = std::time::Instant::now();
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(dag.len(), 1000);
        assert!(elapsed.as_millis() < 100);

        // All leaves ready after root done.
        let mut statuses: HashMap<String, Status> = HashMap::new();
        statuses.insert("root".to_string(), Status::Done);
        for i in 0..999 {
            statuses.insert(format!("leaf-{i}"), Status::Todo);
        }
        let ready = dag.ready_tasks(&statuses);
        assert_eq!(ready.len(), 999);
    }

    // ── topological_sort ────────────────────────────────────────────

    #[test]
    fn test_topological_sort_linear() {
        let tasks = vec![
            make_task("a", &[]),
            make_task("b", &["a"]),
            make_task("c", &["b"]),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let order: Vec<String> = dag
            .topological_sort()
            .iter()
            .map(|&ni| dag.graph[ni].clone())
            .collect();

        // a must come before b, b before c.
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_b = order.iter().position(|x| x == "b").unwrap();
        let pos_c = order.iter().position(|x| x == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }
}
