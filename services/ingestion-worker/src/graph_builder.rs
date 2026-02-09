//! Dependency Graph Builder
//! 
//! Builds an in-memory graph from parsed code with symbol resolution
//! and cross-file dependency tracking.

use crate::parsers::{FunctionInfo, ParsedFile};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Node and Edge Types
// ============================================================================

/// Unique identifier for a node in the dependency graph
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeId {
    /// A source file
    File(String),
    /// A class/struct (file_path, class_name)
    Class(String, String),
    /// A function/method (file_path, func_name)
    Function(String, String),
    /// An external module (import path)
    Module(String),
}

impl NodeId {
    pub fn file_path(&self) -> Option<&str> {
        match self {
            NodeId::File(p) => Some(p),
            NodeId::Class(p, _) => Some(p),
            NodeId::Function(p, _) => Some(p),
            NodeId::Module(_) => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            NodeId::File(p) => p,
            NodeId::Class(_, n) => n,
            NodeId::Function(_, n) => n,
            NodeId::Module(m) => m,
        }
    }

    pub fn node_type(&self) -> &'static str {
        match self {
            NodeId::File(_) => "File",
            NodeId::Class(_, _) => "Class",
            NodeId::Function(_, _) => "Function",
            NodeId::Module(_) => "Module",
        }
    }
}

/// Types of relationships between nodes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeType {
    /// File defines a class or function
    Defines,
    /// Function calls another function
    Calls,
    /// File imports a module or another file
    Imports,
    /// Class inherits from another class
    Inherits,
    /// Class contains a method
    Contains,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::Defines => "DEFINES",
            EdgeType::Calls => "CALLS",
            EdgeType::Imports => "IMPORTS",
            EdgeType::Inherits => "INHERITS",
            EdgeType::Contains => "CONTAINS",
        }
    }
}

/// An edge in the dependency graph
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub edge_type: EdgeType,
    pub properties: HashMap<String, String>,
}

// ============================================================================
// Symbol Table
// ============================================================================

/// Symbol entry with metadata
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    pub file_path: String,
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// Index of all symbols in the codebase for lookup
#[derive(Debug, Default)]
pub struct SymbolTable {
    /// Maps function name -> list of locations where it's defined
    pub functions: HashMap<String, Vec<SymbolEntry>>,
    /// Maps class name -> list of locations where it's defined
    pub classes: HashMap<String, Vec<SymbolEntry>>,
    /// Maps file path -> list of symbol names exported
    pub file_exports: HashMap<String, Vec<String>>,
    /// Maps file path -> ParsedFile reference data
    pub files: HashMap<String, FileSymbols>,
}

/// Symbols defined in a single file
#[derive(Debug, Clone, Default)]
pub struct FileSymbols {
    pub functions: Vec<String>,
    pub classes: Vec<String>,
    pub imports: Vec<String>,
}

impl SymbolTable {
    /// Build a symbol table from parsed files
    pub fn from_parsed_files(parsed_files: &[ParsedFile]) -> Self {
        let mut table = SymbolTable::default();

        for file in parsed_files {
            let mut file_symbols = FileSymbols::default();

            // Index functions
            for func in &file.functions {
                let entry = SymbolEntry {
                    file_path: file.path.clone(),
                    name: func.name.clone(),
                    start_line: func.start_line,
                    end_line: func.end_line,
                };
                table
                    .functions
                    .entry(func.name.clone())
                    .or_default()
                    .push(entry);
                file_symbols.functions.push(func.name.clone());
            }

            // Index classes and their methods
            for class in &file.classes {
                let entry = SymbolEntry {
                    file_path: file.path.clone(),
                    name: class.name.clone(),
                    start_line: class.start_line,
                    end_line: class.end_line,
                };
                table
                    .classes
                    .entry(class.name.clone())
                    .or_default()
                    .push(entry);
                file_symbols.classes.push(class.name.clone());

                // Index methods
                for method in &class.methods {
                    let qualified_name = format!("{}.{}", class.name, method.name);
                    let method_entry = SymbolEntry {
                        file_path: file.path.clone(),
                        name: method.name.clone(),
                        start_line: method.start_line,
                        end_line: method.end_line,
                    };
                    table
                        .functions
                        .entry(qualified_name)
                        .or_default()
                        .push(method_entry.clone());
                    // Also index by simple name for cross-file resolution
                    table
                        .functions
                        .entry(method.name.clone())
                        .or_default()
                        .push(method_entry);
                }
            }

            // Index imports
            file_symbols.imports = file.imports.clone();

            // Build exports list
            let mut exports = file_symbols.functions.clone();
            exports.extend(file_symbols.classes.clone());
            table.file_exports.insert(file.path.clone(), exports);
            table.files.insert(file.path.clone(), file_symbols);
        }

        table
    }

    /// Resolve a function call to its definition(s)
    pub fn resolve_function(&self, name: &str, current_file: &str) -> Option<&SymbolEntry> {
        if let Some(entries) = self.functions.get(name) {
            // Prefer definition in the same file
            if let Some(same_file) = entries.iter().find(|e| e.file_path == current_file) {
                return Some(same_file);
            }
            // Otherwise return the first one found
            entries.first()
        } else {
            None
        }
    }

    /// Resolve a class reference to its definition
    pub fn resolve_class(&self, name: &str, current_file: &str) -> Option<&SymbolEntry> {
        if let Some(entries) = self.classes.get(name) {
            if let Some(same_file) = entries.iter().find(|e| e.file_path == current_file) {
                return Some(same_file);
            }
            entries.first()
        } else {
            None
        }
    }
}

// ============================================================================
// Dependency Graph
// ============================================================================

/// The complete dependency graph
#[derive(Debug, Default)]
pub struct DependencyGraph {
    pub nodes: HashSet<NodeId>,
    pub edges: Vec<Edge>,
}

impl DependencyGraph {
    /// Build a dependency graph from parsed files
    pub fn from_parsed_files(parsed_files: &[ParsedFile], symbol_table: &SymbolTable) -> Self {
        let mut graph = DependencyGraph::default();

        for file in parsed_files {
            let file_node = NodeId::File(file.path.clone());
            graph.nodes.insert(file_node.clone());

            // Process top-level functions
            for func in &file.functions {
                let func_node = NodeId::Function(file.path.clone(), func.name.clone());
                graph.nodes.insert(func_node.clone());

                // File DEFINES Function
                graph.edges.push(Edge {
                    from: file_node.clone(),
                    to: func_node.clone(),
                    edge_type: EdgeType::Defines,
                    properties: HashMap::new(),
                });

                // Process function calls
                graph.add_call_edges(&func_node, func, &file.path, symbol_table);
            }

            // Process classes
            for class in &file.classes {
                let class_node = NodeId::Class(file.path.clone(), class.name.clone());
                graph.nodes.insert(class_node.clone());

                // File DEFINES Class
                graph.edges.push(Edge {
                    from: file_node.clone(),
                    to: class_node.clone(),
                    edge_type: EdgeType::Defines,
                    properties: HashMap::new(),
                });

                // Process inheritance
                for inheritance in &class.inheritances {
                    if let Some(parent_entry) = symbol_table.resolve_class(&inheritance.name, &file.path) {
                        let parent_node =
                            NodeId::Class(parent_entry.file_path.clone(), inheritance.name.clone());
                        graph.nodes.insert(parent_node.clone());
                        let mut properties = HashMap::new();
                        properties.insert("kind".to_string(), inheritance.kind.clone());
                        graph.edges.push(Edge {
                            from: class_node.clone(),
                            to: parent_node,
                            edge_type: EdgeType::Inherits,
                            properties,
                        });
                    } else {
                        // External parent class - create a module node
                        let parent_node = NodeId::Module(inheritance.name.clone());
                        graph.nodes.insert(parent_node.clone());
                        let mut properties = HashMap::new();
                        properties.insert("kind".to_string(), inheritance.kind.clone());
                        graph.edges.push(Edge {
                            from: class_node.clone(),
                            to: parent_node,
                            edge_type: EdgeType::Inherits,
                            properties,
                        });
                    }
                }

                // Process methods
                for method in &class.methods {
                    let method_node = NodeId::Function(file.path.clone(), method.name.clone());
                    graph.nodes.insert(method_node.clone());

                    // Class CONTAINS Method
                    graph.edges.push(Edge {
                        from: class_node.clone(),
                        to: method_node.clone(),
                        edge_type: EdgeType::Contains,
                        properties: HashMap::new(),
                    });

                    // Process method calls
                    graph.add_call_edges(&method_node, method, &file.path, symbol_table);
                }
            }

            // Process imports
            for import in &file.imports {
                let module_node = NodeId::Module(import.clone());
                graph.nodes.insert(module_node.clone());

                graph.edges.push(Edge {
                    from: file_node.clone(),
                    to: module_node,
                    edge_type: EdgeType::Imports,
                    properties: HashMap::new(),
                });
            }
        }

        graph
    }

    /// Add CALLS edges from a function to its callees
    fn add_call_edges(
        &mut self,
        caller_node: &NodeId,
        func: &FunctionInfo,
        current_file: &str,
        symbol_table: &SymbolTable,
    ) {
        for call in &func.calls {
            if let Some(callee_entry) = symbol_table.resolve_function(call, current_file) {
                let callee_node =
                    NodeId::Function(callee_entry.file_path.clone(), callee_entry.name.clone());
                self.nodes.insert(callee_node.clone());
                self.edges.push(Edge {
                    from: caller_node.clone(),
                    to: callee_node,
                    edge_type: EdgeType::Calls,
                    properties: HashMap::new(),
                });
            }
            // If unresolved, we skip - it's likely an external/built-in function
        }
    }

    /// Get all edges of a specific type
    pub fn edges_of_type(&self, edge_type: EdgeType) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.edge_type == edge_type).collect()
    }

    /// Get statistics about the graph
    pub fn stats(&self) -> GraphStats {
        let mut stats = GraphStats::default();
        for node in &self.nodes {
            match node {
                NodeId::File(_) => stats.files += 1,
                NodeId::Class(_, _) => stats.classes += 1,
                NodeId::Function(_, _) => stats.functions += 1,
                NodeId::Module(_) => stats.modules += 1,
            }
        }
        for edge in &self.edges {
            match edge.edge_type {
                EdgeType::Defines => stats.defines_edges += 1,
                EdgeType::Calls => stats.calls_edges += 1,
                EdgeType::Imports => stats.imports_edges += 1,
                EdgeType::Inherits => stats.inherits_edges += 1,
                EdgeType::Contains => stats.contains_edges += 1,
            }
        }
        stats
    }
}

#[derive(Debug, Default)]
pub struct GraphStats {
    pub files: usize,
    pub classes: usize,
    pub functions: usize,
    pub modules: usize,
    pub defines_edges: usize,
    pub calls_edges: usize,
    pub imports_edges: usize,
    pub inherits_edges: usize,
    pub contains_edges: usize,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{ClassInfo, FunctionInfo, InheritanceInfo, ParsedFile};

    fn make_func(name: &str, calls: Vec<&str>) -> FunctionInfo {
        FunctionInfo {
            name: name.to_string(),
            params: vec![],
            return_type: None,
            calls: calls.into_iter().map(String::from).collect(),
            start_line: 1,
            end_line: 10,
        }
    }

    fn make_class(name: &str, parents: Vec<&str>, methods: Vec<FunctionInfo>) -> ClassInfo {
        ClassInfo {
            name: name.to_string(),
            inheritances: parents
                .into_iter()
                .map(|p| InheritanceInfo {
                    name: p.to_string(),
                    kind: "class".to_string(),
                })
                .collect(),
            methods,
            start_line: 1,
            end_line: 50,
        }
    }

    #[test]
    fn test_symbol_table_construction() {
        let files = vec![
            ParsedFile {
                path: "file_a.rs".to_string(),
                language: "rust".to_string(),
                functions: vec![make_func("foo", vec!["bar"])],
                classes: vec![],
                imports: vec![],
                data_tables: vec![],
                service_calls: vec![],
            },
            ParsedFile {
                path: "file_b.rs".to_string(),
                language: "rust".to_string(),
                functions: vec![make_func("bar", vec![])],
                classes: vec![],
                imports: vec![],
                data_tables: vec![],
                service_calls: vec![],
            },
        ];

        let table = SymbolTable::from_parsed_files(&files);

        assert!(table.functions.contains_key("foo"));
        assert!(table.functions.contains_key("bar"));
        assert_eq!(table.functions.get("foo").unwrap().len(), 1);
    }

    #[test]
    fn test_cross_file_call_resolution() {
        let files = vec![
            ParsedFile {
                path: "caller.rs".to_string(),
                language: "rust".to_string(),
                functions: vec![make_func("main", vec!["helper"])],
                classes: vec![],
                imports: vec![],
                data_tables: vec![],
                service_calls: vec![],
            },
            ParsedFile {
                path: "callee.rs".to_string(),
                language: "rust".to_string(),
                functions: vec![make_func("helper", vec![])],
                classes: vec![],
                imports: vec![],
                data_tables: vec![],
                service_calls: vec![],
            },
        ];

        let table = SymbolTable::from_parsed_files(&files);
        let graph = DependencyGraph::from_parsed_files(&files, &table);

        let calls = graph.edges_of_type(EdgeType::Calls);
        assert_eq!(calls.len(), 1);
        assert!(matches!(&calls[0].from, NodeId::Function(f, n) if f == "caller.rs" && n == "main"));
        assert!(matches!(&calls[0].to, NodeId::Function(f, n) if f == "callee.rs" && n == "helper"));
    }

    #[test]
    fn test_inheritance_edges() {
        let files = vec![ParsedFile {
            path: "models.py".to_string(),
            language: "python".to_string(),
            functions: vec![],
            classes: vec![
                make_class("Animal", vec![], vec![]),
                make_class("Dog", vec!["Animal"], vec![make_func("bark", vec![])]),
            ],
            imports: vec![],
            data_tables: vec![],
            service_calls: vec![],
        }];

        let table = SymbolTable::from_parsed_files(&files);
        let graph = DependencyGraph::from_parsed_files(&files, &table);

        let inherits = graph.edges_of_type(EdgeType::Inherits);
        assert_eq!(inherits.len(), 1);
        assert!(matches!(&inherits[0].from, NodeId::Class(_, n) if n == "Dog"));
        assert!(matches!(&inherits[0].to, NodeId::Class(_, n) if n == "Animal"));
    }
}
