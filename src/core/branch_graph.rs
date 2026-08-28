use std::collections::{HashMap, HashSet};
use std::iter::Rev;
use std::ops::Range;
use std::rc::Rc;

use anyhow::{Context, Result};
use git2::{Branch, Repository};
use petgraph::algo::scc::tarjan_scc;
use petgraph::algo::toposort;
use petgraph::graph::NodeIndex;
use petgraph::stable_graph::StableDiGraph;

use crate::core::NotFoundExt;
use crate::core::branch_info::BranchInfo;
use crate::core::user_config::UserConfig;

/// A graph of branches. Feature's base branches creates a dependency
/// relationship between branches. While this should generally be a tree,
/// there's no guarantee that it will be. A graph is used to build the entire
/// dependency graph first, then handle cycles as needed (either erroring or
/// removing the cycle).
///
/// This graph loads all branches, and creates directed edges from a dependency
/// branch to a dependent branch (parent to child).
///
/// > Note: the terms "base", "dependency", and "parent" are used
/// > interchagneably. Same with the terms "dependent" and "child".
///
/// You can obtain iterators that sort in topological order (parent to child,
/// starting from the root) or reverse topological order (child to parent,
/// starting from the leaves) in order to walk the dependency graph to perform
/// ordered operations.
///
/// For example, syncing a branch requires that its dependencies are updated
/// first. Pruning all branches requires that dependent branches (leaves) are
/// deleted first. Deleting branches requires that a dependency is never
/// deleted.
#[derive(Debug)]
pub struct BranchGraph {
  /// Graph of branches
  graph: InnerGraph,

  /// Maps branch names (shorthand) to indices in the graph
  map: NodeTable,
}

type InnerGraph = StableDiGraph<Rc<str>, ()>;
type NodeTable = HashMap<Rc<str>, NodeIndex>;
type NodeSet = HashSet<NodeIndex>;

impl BranchGraph {
  /// Load the branch graph from the given repo. This is a very expensive
  /// computation. Avoid loading more than once if possible.
  pub fn load(repo: &Repository) -> Result<Self> {
    let mut this = Self {
      graph: StableDiGraph::new(),
      map: HashMap::new(),
    };

    let config = UserConfig::new(repo)?;

    let branches: Vec<Branch> = repo
      .branches(Some(git2::BranchType::Local))?
      .flatten()
      .map(|(b, _)| b)
      .collect();

    // build table to look up a branch by its upstream
    let mut upstream_to_local: HashMap<BranchInfo, &Branch> = HashMap::new();
    for branch in &branches {
      let upstream = branch.upstream().not_found_ok()?;
      if let Some(upstream) = upstream {
        upstream_to_local.insert(BranchInfo::from_branch(&upstream)?, branch);
      }
    }

    for branch in &branches {
      let info = BranchInfo::from_branch(branch)?;
      let branch_name: Rc<str> = Rc::from(info.name());

      this.get_or_insert_node(branch_name.clone());

      let base = config.branch_base(branch.name()?.expect("Branch names must be utf-8"))?;
      if let Some(base) = base {
        // if it's an upstream, and there's a local copy, use the local shortname
        let base_name: Rc<str> = if base.is_remote()
          && let Some(local) = upstream_to_local.get(&base)
        {
          Rc::from(local.name()?.expect("Branch names must be utf-8"))
        } else {
          // else use the remote shortname, e.g. remote/branch-name
          Rc::from(base.name())
        };

        this.add_edge(base_name, branch_name.clone());
      }
    }

    Ok(this)
  }

  /// Remove a branch from the dependency graph. This should be done to keep the
  /// graph updated when pruning a branch.
  pub fn remove_branch(&mut self, branch: &str) -> Result<()> {
    let index = self
      .map
      .remove(branch)
      .with_context(|| format!("Branch '{}' not found in graph", branch))?;

    self.graph.remove_node(index);
    Ok(())
  }

  /// Sets a branch as a dependency of another
  ///
  /// # Errors
  /// If the added edge creates a cycle
  pub fn add_dependency<'names>(
    &mut self,
    parent: &'names str,
    child: &'names str,
  ) -> Result<(), BranchGraphError<'names>> {
    let base: Rc<str> = Rc::from(parent);
    let branch: Rc<str> = Rc::from(child);
    self.add_edge(base, branch);

    if toposort(&self.graph, None).is_err() {
      Err(BranchGraphError::CycleCreated {
        branch: child,
        base: parent,
      })
    } else {
      Ok(())
    }
  }

  /// Gets an iterator that walks the graph from parent to child (starting from
  /// root).
  pub fn iter_from_root<'graph>(
    &'graph self,
  ) -> Result<BranchGraphIter<'graph>, BranchGraphError<'graph>> {
    let indices = toposort(&self.graph, None).map_err(|_| BranchGraphError::CycleExists {
      cycles: self.get_cycles(),
    })?;

    Ok(BranchGraphIter::new(self, indices))
  }

  /// Gets an iterator that walks the graph from child to parent (starting from
  /// leaves).
  pub fn iter_from_leaves<'graph>(
    &'graph self,
  ) -> Result<Rev<BranchGraphIter<'graph>>, BranchGraphError<'graph>> {
    let indices = toposort(&self.graph, None).map_err(|_| BranchGraphError::CycleExists {
      cycles: self.get_cycles(),
    })?;

    Ok(BranchGraphIter::new(self, indices).rev())
  }

  /// Whether the given branch has a child (dependent) branch.
  pub fn has_child(&self, branch: &str) -> bool {
    let index = self.map[branch];
    self.graph.neighbors(index).next().is_some()
  }

  /// Gets a list of cycles with nodes represented as [NodeIndex]'s.
  fn get_cycles_as_indicies(&self) -> Vec<Vec<NodeIndex>> {
    // get list of strongly connected components. this is supposed to respect
    // directed edges but i don't think it does
    let components = tarjan_scc(&self.graph);

    let mut paths = Vec::new();

    // cycles need to be ordered. use a depth first search
    for comp in &components {
      let members = comp.iter().copied().collect::<NodeSet>();
      let start = comp[0];

      let mut visited = NodeSet::from([start]);
      let mut path = vec![start];

      self.dfs(&members, start, start, &mut visited, &mut path);

      if path.len() > 1 {
        paths.push(path);
      }
    }

    paths
  }

  /// Gets a list of each cycle in the graph
  ///
  /// # Lifetimes
  /// - `names` - the lifetime of the string references, which must be outlived
  ///   by the graph
  pub fn get_cycles(&self) -> Vec<Vec<&str>> {
    let cycles = self.get_cycles_as_indicies();
    let mut out = Vec::with_capacity(cycles.len());

    // map indices to branch names
    for cycle in &cycles {
      let path: Vec<&str> = cycle
        .iter()
        .map(|index| self.index_to_branch(*index))
        .collect();

      if path.len() > 1 {
        out.push(path);
      }
    }

    out
  }

  /// Remove all nodes that are contained within a cycle. This mutates the graph
  /// in place, resulting in an acyclic graph with only the branches not in a
  /// cycle. This allows the user to sync/prune without having to resolve every
  /// branch dependency issue.
  ///
  /// # Returns
  /// A list of each removed node. These could've been in any of the cycles.
  pub fn remove_cycles(&mut self) -> Vec<String> {
    let cycles = self.get_cycles_as_indicies();

    let mut removed = HashSet::new();

    // remove every offending node
    for cycle in &cycles {
      for index in cycle {
        removed.insert(self.graph.remove_node(*index));
      }
    }

    // these must be owned, they're no longer contained in the graph
    removed.iter().flatten().map(ToString::to_string).collect()
  }

  /// Traverses `members` depth-first
  ///
  /// # Params
  /// - `visited` - the set of visited nodes. this is modified internally
  /// - `members` - the set of nodes in the connected component
  /// - `current` - the node currently being visited
  /// - `start` - the starting node of the traversal
  /// - `path` - the output list of ordered nodes
  ///
  /// # Returns
  /// `true` if the loop was closed, `false` otherwise
  fn dfs(
    &self,
    members: &NodeSet,
    start: NodeIndex,
    current: NodeIndex,
    visited: &mut NodeSet,
    path: &mut Vec<NodeIndex>,
  ) -> bool {
    for neighbor in self
      .graph
      .neighbors_directed(current, petgraph::Direction::Outgoing)
    {
      if !members.contains(&neighbor) {
        // skip nodes outside the connected component
        continue;
      }

      if neighbor == start && path.len() > 1 {
        // loop complete
        path.push(start); // intentionally pushing the start element again
        return true;
      }

      if visited.insert(neighbor) {
        // neighbor was not previously visited
        path.push(neighbor);

        // traverse deeper
        if self.dfs(members, start, neighbor, visited, path) {
          // loop was closed
          return true;
        }

        // loop was never closed
        path.pop();
        visited.remove(&neighbor);
      }
    }

    false
  }

  fn index_to_branch(&self, index: NodeIndex) -> &str {
    Rc::as_ref(&self.graph[index])
  }

  /// Inserts a branch into the graph. If the branch is was already added, this
  /// just returns the graph index.
  ///
  /// # Params
  /// - `branch` - the shorthand name of the branch
  ///
  /// # Returns
  /// The graph index of the branch
  fn get_or_insert_node(&mut self, branch: Rc<str>) -> NodeIndex {
    if !self.map.contains_key(&branch) {
      let node = self.graph.add_node(branch.clone());
      self.map.insert(branch.clone(), node);
      node
    } else {
      self.map[&branch]
    }
  }

  /// Adds an edge pointing from `parent` to `child`. Ensures that `parent` and
  /// `child` both exist in the graph.
  ///
  /// # Params
  /// - `parent` - the parent (base) branch's refname
  /// - `child` - the child branch's refname
  fn add_edge(&mut self, parent: Rc<str>, child: Rc<str>) {
    let a = self.get_or_insert_node(parent);
    let b = self.get_or_insert_node(child);
    self.graph.add_edge(a, b, ());
  }
}

/// # Lifetimes
/// - `names` - the lifetime of the string references, representing branch names
#[derive(Debug)]
pub enum BranchGraphError<'names> {
  /// A cycle exists in the graph, preventing it from being walked
  CycleExists { cycles: Vec<Vec<&'names str>> },

  /// A cycle would be created by the insertion
  CycleCreated {
    branch: &'names str,
    base: &'names str,
  },
}

pub struct BranchGraphIter<'graph> {
  graph: &'graph BranchGraph,
  indices: Vec<NodeIndex>,
  range: Range<usize>,
}

impl<'graph> BranchGraphIter<'graph> {
  fn new(graph: &'graph BranchGraph, indices: Vec<NodeIndex>) -> Self {
    let range = 0..indices.len();
    Self {
      graph,
      indices,
      range,
    }
  }
}

impl<'graph> Iterator for BranchGraphIter<'graph> {
  type Item = &'graph str;

  fn next(&mut self) -> Option<Self::Item> {
    self.range.next().map(|i| {
      let node = self.indices[i];
      let node = &self.graph.graph[node];
      Rc::as_ref(node)
    })
  }
}

impl<'graph> DoubleEndedIterator for BranchGraphIter<'graph> {
  fn next_back(&mut self) -> Option<Self::Item> {
    self.range.next_back().map(|i| {
      let node = self.indices[i];
      let node = &self.graph.graph[node];
      Rc::as_ref(node)
    })
  }
}
