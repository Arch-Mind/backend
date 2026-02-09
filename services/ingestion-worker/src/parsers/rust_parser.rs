use super::{ClassInfo, FunctionInfo, InheritanceInfo, LanguageParser, ParsedFile, ServiceCall};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tree_sitter::{Node, Parser, Query, QueryCursor};

pub struct RustParser;

impl RustParser {
    pub fn new() -> Result<Self> {
        Ok(RustParser)
    }

    fn extract_data_tables(&self, content: &str) -> Vec<String> {
        let mut tables = HashSet::new();
        let patterns = [
            r"(?i)\bfrom\s+([a-zA-Z0-9_.]+)",
            r"(?i)\bjoin\s+([a-zA-Z0-9_.]+)",
            r"(?i)\binto\s+([a-zA-Z0-9_.]+)",
            r"(?i)\bupdate\s+([a-zA-Z0-9_.]+)",
            r"(?i)\bdelete\s+from\s+([a-zA-Z0-9_.]+)",
            r#"(?i)\btable\(\s*['"]([a-zA-Z0-9_.]+)['"]"#,
        ];

        for pattern in patterns {
            if let Ok(re) = Regex::new(pattern) {
                for cap in re.captures_iter(content) {
                    if let Some(m) = cap.get(1) {
                        tables.insert(m.as_str().to_string());
                    }
                }
            }
        }

        tables.into_iter().collect()
    }

    fn extract_service_calls(&self, content: &str) -> Vec<ServiceCall> {
        let mut services = HashSet::new();
        let url_pattern = r#"(?i)\b(https?|grpc)://[^\s'"`]+"#;

        if let Ok(re) = Regex::new(url_pattern) {
            for cap in re.captures_iter(content) {
                let full = cap.get(0).map(|m| m.as_str()).unwrap_or_default();
                let protocol = cap.get(1).map(|m| m.as_str()).unwrap_or("http");
                if let Some(target) = extract_service_target(full) {
                    services.insert((target, protocol.to_string()));
                }
            }
        }

        services
            .into_iter()
            .map(|(target, protocol)| ServiceCall { target, protocol })
            .collect()
    }

    fn extract_params(&self, node: Node, content: &str) -> Vec<String> {
        let mut params = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
             if child.kind() == "parameter" {
                 if let Some(pattern) = child.child_by_field_name("pattern") {
                     params.push(content[pattern.byte_range()].to_string());
                 }
             } else if child.kind() == "self_parameter" {
                 params.push("self".to_string());
             }
        }
        params
    }

    fn extract_calls(&self, node: Node, content: &str, query: &Query) -> Vec<String> {
        let mut calls = HashSet::new();
        let mut query_cursor = QueryCursor::new();
        let matches = query_cursor.matches(query, node, content.as_bytes());
        for m in matches {
            for capture in m.captures {
                 let capture_name = &query.capture_names()[capture.index as usize];
                 if capture_name == "call.name" {
                     let call_name = content[capture.node.byte_range()].to_string();
                     calls.insert(call_name);
                 }
            }
        }
        calls.into_iter().collect()
    }
}

impl LanguageParser for RustParser {
    fn parse_file(&self, path: &PathBuf, content: &str) -> Result<ParsedFile> {
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_rust::language())
            .context("Failed to set Rust language")?;
        let tree = parser
            .parse(content, None)
            .context("Failed to parse Rust file")?;

        let root_node = tree.root_node();
        let mut functions = Vec::new();
        let mut class_map: HashMap<String, ClassInfo> = HashMap::new();
        let mut imports = Vec::new();

        // Queries
        let function_query = Query::new(
            tree_sitter_rust::language(),
            r#"
            (function_item
              name: (identifier) @func.name
              parameters: (parameters) @func.params
              body: (block) @func.body
            ) @func.def
            "#,
        )?;

        let struct_query = Query::new(
            tree_sitter_rust::language(),
            r#"
            (struct_item name: (type_identifier) @name) @def
            (enum_item name: (type_identifier) @name) @def
            "#,
        )?;

                let impl_query = Query::new(
            tree_sitter_rust::language(),
            r#"
            (impl_item
              type: (type_identifier) @target
              body: (declaration_list) @body
            ) @impl
            "#,
        )?;

                let trait_impl_query = Query::new(
                        tree_sitter_rust::language(),
                        r#"
                        (impl_item
                            trait: (type_identifier) @trait
                            type: (type_identifier) @target
                        ) @impl_trait
                        "#,
                )?;

        let call_query = Query::new(
            tree_sitter_rust::language(),
            r#"
            (call_expression
              function: [
                (identifier) @call.name
                (field_expression field: (field_identifier) @call.name)
                (scoped_identifier name: (identifier) @call.name)
              ])
            "#,
        )?;

        let import_query = Query::new(
            tree_sitter_rust::language(),
            r#"
            (use_declaration
                argument: [(scoped_identifier) (identifier) (use_wildcard)] @import.source
            )
            "#,
        )?;

        let mut query_cursor = QueryCursor::new();

        // 1. Extract Structs/Enums
        let str_matches = query_cursor.matches(&struct_query, root_node, content.as_bytes());
        for m in str_matches {
            let mut name = String::new();
            let mut node = root_node;
            for c in m.captures {
                let cn = &struct_query.capture_names()[c.index as usize];
                if cn == "name" {
                    name = content[c.node.byte_range()].to_string();
                } else if cn == "def" {
                    node = c.node;
                }
            }
            if !name.is_empty() {
                 class_map.insert(name.clone(), ClassInfo {
                     name,
                     inheritances: Vec::new(),
                     methods: Vec::new(),
                     start_line: node.start_position().row + 1,
                     end_line: node.end_position().row + 1,
                 });
            }
        }

        // 1b. Extract Trait Implementations
        let trait_impl_matches = query_cursor.matches(&trait_impl_query, root_node, content.as_bytes());
        for m in trait_impl_matches {
            let mut trait_name = String::new();
            let mut target_name = String::new();

            for c in m.captures {
                let cn = &trait_impl_query.capture_names()[c.index as usize];
                if cn == "trait" {
                    trait_name = content[c.node.byte_range()].to_string();
                } else if cn == "target" {
                    target_name = content[c.node.byte_range()].to_string();
                }
            }

            if !trait_name.is_empty() && !target_name.is_empty() {
                let entry = class_map.entry(target_name.clone()).or_insert(ClassInfo {
                    name: target_name,
                    inheritances: Vec::new(),
                    methods: Vec::new(),
                    start_line: 0,
                    end_line: 0,
                });
                entry.inheritances.push(InheritanceInfo {
                    name: trait_name,
                    kind: "trait".to_string(),
                });
            }
        }

        // 2. Extract Impl blocks
        let impl_matches = query_cursor.matches(&impl_query, root_node, content.as_bytes());
        for m in impl_matches {
            let mut target_name = String::new();
            let mut body_node = root_node;
            
            for c in m.captures {
                let cn = &impl_query.capture_names()[c.index as usize];
                if cn == "target" {
                    target_name = content[c.node.byte_range()].to_string();
                } else if cn == "body" {
                    body_node = c.node;
                }
            }

            if !target_name.is_empty() {
                 let mut class_info = class_map.remove(&target_name).unwrap_or(ClassInfo {
                     name: target_name.clone(),
                     inheritances: Vec::new(),
                     methods: Vec::new(),
                     start_line: 0,
                     end_line: 0,
                 });
                 
                 let mut method_cursor = QueryCursor::new();
                 let method_matches = method_cursor.matches(&function_query, body_node, content.as_bytes());
                 for mm in method_matches {
                     let mut m_name = String::new();
                     let mut m_node = root_node;
                     let mut m_params_node = None;
                     
                     for c in mm.captures {
                         let cn = &function_query.capture_names()[c.index as usize];
                         if cn == "func.name" {
                             m_name = content[c.node.byte_range()].to_string();
                         } else if cn == "func.def" {
                             m_node = c.node;
                         } else if cn == "func.params" {
                             m_params_node = Some(c.node);
                         }
                     }
                     
                     if !m_name.is_empty() {
                         let params = if let Some(pn) = m_params_node {
                             self.extract_params(pn, content)
                         } else {
                             Vec::new()
                         };
                         let calls = self.extract_calls(m_node, content, &call_query);
                         
                         class_info.methods.push(FunctionInfo {
                             name: m_name,
                             params,
                             return_type: None, 
                             calls,
                             start_line: m_node.start_position().row + 1,
                             end_line: m_node.end_position().row + 1,
                         });
                     }
                 }
                 
                 class_map.insert(target_name, class_info);
            }
        }

        // 3. Extract Top Level Functions
        let func_matches = query_cursor.matches(&function_query, root_node, content.as_bytes());
        for m in func_matches {
             let mut name = String::new();
             let mut node = root_node;
             let mut params_node = None; 
             
             for c in m.captures {
                let cn = &function_query.capture_names()[c.index as usize];
                if cn == "func.name" {
                    name = content[c.node.byte_range()].to_string();
                } else if cn == "func.def" {
                    node = c.node;
                } else if cn == "func.params" {
                    params_node = Some(c.node);
                }
             }

             if let Some(parent) = node.parent() {
                 if parent.kind() == "source_file" || parent.kind() == "mod_item" {
                     let params = if let Some(pn) = params_node {
                         self.extract_params(pn, content)
                     } else {
                         Vec::new()
                     };
                     let calls = self.extract_calls(node, content, &call_query);
                     
                     functions.push(FunctionInfo {
                         name,
                         params,
                         return_type: None,
                         calls,
                         start_line: node.start_position().row + 1,
                         end_line: node.end_position().row + 1,
                     });
                 }
             }
        }

        // 4. Imports
        let import_matches = query_cursor.matches(&import_query, root_node, content.as_bytes());
        for m in import_matches {
            for c in m.captures {
                let cn = &import_query.capture_names()[c.index as usize];
                if cn == "import.source" {
                    imports.push(content[c.node.byte_range()].to_string());
                }
            }
        }

        let data_tables = self.extract_data_tables(content);
        let service_calls = self.extract_service_calls(content);

        Ok(ParsedFile {
            path: path.to_string_lossy().to_string(),
            language: "rust".to_string(),
            functions,
            classes: class_map.into_values().collect(),
            imports,
            data_tables,
            service_calls,
        })
    }
}

fn extract_service_target(url: &str) -> Option<String> {
    let parts: Vec<&str> = url.split("//").collect();
    let host_part = parts.get(1).copied().unwrap_or("");
    let host = host_part.split('/').next().unwrap_or("");
    let host = host.split('?').next().unwrap_or("");
    let host = host.split('#').next().unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_full() {
        let parser = RustParser::new().unwrap();
        let content = r#"
            use std::collections::HashMap;
            use serde::{Serialize, Deserialize};

            struct User {
                name: String,
                age: u32
            }

            impl User {
                fn new(name: String) -> Self {
                    Self { name, age: 0 }
                }

                fn grow(&mut self) {
                    self.age += 1;
                    println!("Grew!");
                }
            }

            fn main() {
                let mut u = User::new("Alice".to_string());
                u.grow();
            }
        "#;
        
        let result = parser.parse_file(&PathBuf::from("test.rs"), content).unwrap();
        
        // Imports
        assert!(result.imports.iter().any(|i| i.contains("std::collections::HashMap")));
        
        // Structs
        let user = result.classes.iter().find(|c| c.name == "User").expect("User struct not found");
        assert!(user.methods.iter().any(|m| m.name == "new"));
        let grow = user.methods.iter().find(|m| m.name == "grow").unwrap();
        assert_eq!(grow.params, vec!["self"]);
        assert!(grow.calls.contains(&"println!".to_string())); // Note: println! might be identifier
        
        // Functions
        let main = result.functions.iter().find(|f| f.name == "main").expect("main not found");
        assert!(main.calls.contains(&"new".to_string()));
        assert!(main.calls.contains(&"grow".to_string()));
    }
}
