/// Dependency Resolver - manages persona dependencies and execution order
/// 
/// This module provides dependency graph management and topological sorting
/// to ensure personas are executed in the correct order.

use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};

use super::persona::PersonaConfig;

/// Dependency graph node
#[derive(Debug, Clone)]
pub struct DependencyNode {
    pub name: String,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>, // Nodes that depend on this node
}

/// Dependency graph for persona execution order
pub struct DependencyGraph {
    nodes: HashMap<String, DependencyNode>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }
    
    /// Add a node to the graph
    pub fn add_node(&mut self, name: &str, dependencies: Vec<String>) {
        // Remove if exists (update)
        self.nodes.remove(name);
        
        let node = DependencyNode {
            name: name.to_string(),
            dependencies: dependencies.clone(),
            dependents: Vec::new(),
        };
        
        self.nodes.insert(name.to_string(), node);
        
        // Update dependents for each dependency
        for dep in dependencies {
            if let Some(dep_node) = self.nodes.get_mut(&dep) {
                dep_node.dependents.push(name.to_string());
            } else {
                // Create placeholder node for dependency
                let placeholder = DependencyNode {
                    name: dep.clone(),
                    dependencies: Vec::new(),
                    dependents: vec![name.to_string()],
                };
                self.nodes.insert(dep, placeholder);
            }
        }
    }
    
    /// Remove a node from the graph
    pub fn remove_node(&mut self, name: &str) {
        // Remove from dependents of dependencies
        if let Some(node) = self.nodes.get(name) {
            let deps = node.dependencies.clone();
            for dep in deps {
                if let Some(dep_node) = self.nodes.get_mut(&dep) {
                    dep_node.dependents.retain(|d| d != name);
                }
            }
        }
        
        // Remove from dependencies of dependents
        if let Some(node) = self.nodes.remove(name) {
            let dependents = node.dependents;
            for dependent in dependents {
                if let Some(dep_node) = self.nodes.get_mut(&dependent) {
                    dep_node.dependencies.retain(|d| d != name);
                }
            }
        }
    }
    
    /// Get all nodes
    pub fn nodes(&self) -> Vec<&DependencyNode> {
        self.nodes.values().collect()
    }
    
    /// Get node by name
    pub fn get_node(&self, name: &str) -> Option<&DependencyNode> {
        self.nodes.get(name)
    }
    
    /// Topological sort using Kahn's algorithm
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj_list: HashMap<String, Vec<String>> = HashMap::new();
        
        // Initialize
        for (name, node) in &self.nodes {
            in_degree.insert(name.clone(), 0);
            adj_list.insert(name.clone(), Vec::new());
        }
        
        // Build adjacency list and calculate in-degrees
        for (name, node) in &self.nodes {
            for dep in &node.dependencies {
                // Only consider dependencies that exist in our graph
                if self.nodes.contains_key(dep) {
                    adj_list.entry(dep.clone()).or_default().push(name.clone());
                    *in_degree.entry(name.clone()).or_insert(0) += 1;
                }
            }
        }
        
        // Find all nodes with in-degree 0
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(name, _)| name.clone())
            .collect();
        
        let mut result = Vec::new();
        
        while let Some(node) = queue.pop_front() {
            result.push(node.clone());
            
            if let Some(neighbors) = adj_list.get(&node) {
                for neighbor in neighbors {
                    let deg = in_degree.get_mut(neighbor).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
        
        // Check for cycles
        if result.len() != self.nodes.len() {
            // Find nodes involved in cycle
            let remaining: HashSet<_> = self.nodes.keys().collect();
            let sorted: HashSet<_> = result.iter().collect();
            let cycle_nodes: Vec<_> = remaining
                .difference(&sorted)
                .map(|s| s.as_str())
                .collect();
            
            anyhow::bail!(
                "Detected circular dependency in dependency graph: {:?}",
                cycle_nodes
            );
        }
        
        Ok(result)
    }
    
    /// Get execution levels (for parallel execution)
    pub fn get_execution_levels(&self) -> Result<Vec<Vec<String>>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj_list: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize
        for (name, _node) in &self.nodes {
            in_degree.insert(name.clone(), 0);
            adj_list.insert(name.clone(), Vec::new());
        }

        // Build adjacency list and calculate in-degrees
        for (name, node) in &self.nodes {
            for dep in &node.dependencies {
                if self.nodes.contains_key(dep) {
                    adj_list.entry(dep.clone()).or_default().push(name.clone());
                    *in_degree.entry(name.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut levels = Vec::new();
        let mut processed = HashSet::new();

        while processed.len() < self.nodes.len() {
            // Find all nodes with in-degree 0 that haven't been processed
            let current_level: Vec<String> = in_degree
                .iter()
                .filter(|(name, &deg)| deg == 0 && !processed.contains(name.as_str()))
                .map(|(name, _)| name.clone())
                .collect();

            if current_level.is_empty() {
                // No more nodes with in-degree 0, but not all processed = cycle
                let remaining: Vec<String> = self.nodes
                    .keys()
                    .filter(|n| !processed.contains(n.as_str()))
                    .map(|s| s.to_string())
                    .collect();

                anyhow::bail!("Detected circular dependency: {:?}", remaining);
            }

            // Decrease in-degree of neighbors first (before pushing to levels)
            for node in &current_level {
                processed.insert(node.clone());

                // Decrease in-degree of neighbors
                if let Some(neighbors) = adj_list.get(node) {
                    for neighbor in neighbors {
                        let deg = in_degree.get_mut(neighbor).unwrap();
                        *deg -= 1;
                    }
                }
            }

            levels.push(current_level);
        }

        Ok(levels)
    }
    
    /// Validate graph (check for missing dependencies)
    pub fn validate(&self, available_nodes: &[&str]) -> Result<()> {
        let available_set: HashSet<_> = available_nodes.iter().collect();
        
        for (name, node) in &self.nodes {
            for dep in &node.dependencies {
                if !available_set.contains(&dep.as_str()) && !self.nodes.contains_key(dep) {
                    anyhow::bail!(
                        "Node '{}' depends on non-existent node: {}",
                        name,
                        dep
                    );
                }
            }
        }
        
        Ok(())
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Dependency resolver - builds and manages dependency graphs from persona configs
pub struct DependencyResolver {
    graph: DependencyGraph,
}

impl DependencyResolver {
    pub fn new() -> Self {
        Self {
            graph: DependencyGraph::new(),
        }
    }
    
    /// Build dependency graph from persona configs
    pub fn build_from_personas(&mut self, personas: &[PersonaConfig]) -> Result<()> {
        log::info!("Building dependency graph from {} personas", personas.len());
        
        for config in personas {
            let deps = config.dependencies
                .as_ref()
                .map(|d| d.requires.clone())
                .unwrap_or_default();
            
            self.graph.add_node(&config.name, deps);
            
            log::debug!("Added node '{}' with dependencies: {:?}", 
                config.name, 
                config.dependencies.as_ref().map(|d| &d.requires));
        }
        
        Ok(())
    }
    
    /// Resolve execution order
    pub fn resolve(&self) -> Result<Vec<String>> {
        self.graph.topological_sort()
    }
    
    /// Get execution levels for parallel execution
    pub fn get_execution_levels(&self) -> Result<Vec<Vec<String>>> {
        self.graph.get_execution_levels()
    }
    
    /// Validate dependencies
    pub fn validate(&self, available_personas: &[&str]) -> Result<()> {
        self.graph.validate(available_personas)
    }
    
    /// Get the dependency graph
    pub fn graph(&self) -> &DependencyGraph {
        &self.graph
    }
    
    /// Check if a persona can be executed (all dependencies satisfied)
    pub fn can_execute(&self, persona_name: &str, executed: &[&str]) -> bool {
        if let Some(node) = self.graph.get_node(persona_name) {
            for dep in &node.dependencies {
                if self.graph.nodes.contains_key(dep) && !executed.contains(&dep.as_str()) {
                    return false;
                }
            }
            return true;
        }
        false
    }
    
    /// Get all dependencies for a persona (recursive)
    pub fn get_all_dependencies(&self, persona_name: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        
        fn dfs(
            graph: &DependencyGraph,
            name: &str,
            result: &mut Vec<String>,
            visited: &mut HashSet<String>,
        ) {
            if visited.contains(name) {
                return;
            }
            visited.insert(name.to_string());
            
            if let Some(node) = graph.get_node(name) {
                for dep in &node.dependencies {
                    if graph.nodes.contains_key(dep) {
                        result.push(dep.clone());
                        dfs(graph, dep, result, visited);
                    }
                }
            }
        }
        
        dfs(&self.graph, persona_name, &mut result, &mut visited);
        result
    }
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_topological_sort_simple() {
        let mut graph = DependencyGraph::new();
        graph.add_node("A", vec![]);
        graph.add_node("B", vec!["A".to_string()]);
        graph.add_node("C", vec!["B".to_string()]);
        
        let result = graph.topological_sort().unwrap();
        assert_eq!(result, vec!["A", "B", "C"]);
    }
    
    #[test]
    fn test_topological_sort_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_node("A", vec!["C".to_string()]);
        graph.add_node("B", vec!["A".to_string()]);
        graph.add_node("C", vec!["B".to_string()]);
        
        let result = graph.topological_sort();
        assert!(result.is_err());
    }
    
    #[test]
    fn test_execution_levels() {
        let mut graph = DependencyGraph::new();
        graph.add_node("A", vec![]);
        graph.add_node("B", vec![]);
        graph.add_node("C", vec!["A".to_string(), "B".to_string()]);
        
        let levels = graph.get_execution_levels().unwrap();
        assert_eq!(levels.len(), 2);
        assert!(levels[0].contains(&"A".to_string()));
        assert!(levels[0].contains(&"B".to_string()));
        assert_eq!(levels[1], vec!["C".to_string()]);
    }
}
