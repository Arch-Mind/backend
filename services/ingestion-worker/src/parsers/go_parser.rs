use super::{ClassInfo, FunctionInfo, LanguageParser, ParsedFile, ServiceCall};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tree_sitter::{Node, Parser, Query, QueryCursor};

pub struct GoParser;

impl GoParser {
    pub fn new() -> Result<Self> {
        Ok(GoParser)
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
             if child.kind() == "parameter_declaration" {
                 let mut param_cursor = child.walk();
                 for pc in child.children(&mut param_cursor) {
                     if pc.kind() == "identifier" {
                         params.push(content[pc.byte_range()].to_string());
                     }
                 }
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

impl LanguageParser for GoParser {
    fn parse_file(&self, path: &PathBuf, content: &str) -> Result<ParsedFile> {
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_go::language())
            .context("Failed to set Go language")?;
        let tree = parser
            .parse(content, None)
            .context("Failed to parse Go file")?;

        let root_node = tree.root_node();
        let mut functions = Vec::new();
        let mut class_map: HashMap<String, ClassInfo> = HashMap::new();
        let mut imports = Vec::new();

        // queries
        let func_query = Query::new(
            tree_sitter_go::language(),
            r#"
            (function_declaration
              name: (identifier) @func.name
              parameters: (parameter_list) @func.params
              body: (block) @func.body
            ) @func.def

            (method_declaration
              receiver: (parameter_list) @method.receiver
              name: (field_identifier) @func.name
              parameters: (parameter_list) @func.params
              body: (block) @func.body
            ) @func.def
            "#,
        )?;

        let struct_query = Query::new(
            tree_sitter_go::language(),
            r#"
            (type_declaration
              (type_spec
                name: (type_identifier) @name
                type: (struct_type)
              )
            ) @def
            "#,
        )?;

        let call_query = Query::new(
             tree_sitter_go::language(),
             r#"
             (call_expression
               function: [
                 (identifier) @call.name
                 (selector_expression field: (field_identifier) @call.name)
               ]
             )
             "#
        )?;

        let import_query = Query::new(
            tree_sitter_go::language(),
            r#"
            (import_spec path: (string_literal) @import.source)
            "#,
        )?;

        let mut query_cursor = QueryCursor::new();

        // 1. Extract Structs
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

        // 2. Extract Functions and Methods
        let func_matches = query_cursor.matches(&func_query, root_node, content.as_bytes());
        for m in func_matches {
            let mut name = String::new();
            let mut node = root_node;
            let mut params_node = None;
            let mut receiver_node = None;
            
            for c in m.captures {
                 let cn = &func_query.capture_names()[c.index as usize];
                 if cn == "func.name" {
                     name = content[c.node.byte_range()].to_string();
                 } else if cn == "func.def" {
                     node = c.node;
                 } else if cn == "func.params" {
                     params_node = Some(c.node);
                 } else if cn == "method.receiver" {
                     receiver_node = Some(c.node);
                 }
            }
            
            if !name.is_empty() {
                let params = if let Some(pn) = params_node {
                    self.extract_params(pn, content)
                } else {
                    Vec::new()
                };
                let calls = self.extract_calls(node, content, &call_query);
                
                let func_info = FunctionInfo {
                    name: name.clone(),
                    params,
                    return_type: None,
                    calls,
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                };

                if let Some(rn) = receiver_node {
                    // It's a method. We need to find the type name from the receiver
                    let mut receiver_type_name = String::new();
                    
                    let mut rc = rn.walk();
                    for child in rn.children(&mut rc) {
                         if child.kind() == "parameter_declaration" {
                             if let Some(type_node) = child.child_by_field_name("type") {
                                 let type_str = content[type_node.byte_range()].to_string();
                                 receiver_type_name = type_str.replace("*", "").trim().to_string();
                             }
                         }
                    }

                    if !receiver_type_name.is_empty() {
                         let entry = class_map.entry(receiver_type_name.clone()).or_insert(ClassInfo {
                             name: receiver_type_name,
                             inheritances: Vec::new(),
                             methods: Vec::new(),
                             start_line: 0,
                             end_line: 0,
                         });
                         entry.methods.push(func_info);
                    } else {
                         functions.push(func_info);
                    }
                } else {
                     functions.push(func_info);
                }
            }
        }

        // 3. Imports
        let imports_matches = query_cursor.matches(&import_query, root_node, content.as_bytes());
        for m in imports_matches {
             for c in m.captures {
                 let cn = &import_query.capture_names()[c.index as usize];
                 if cn == "import.source" {
                     let imp = content[c.node.byte_range()].trim_matches('"').to_string();
                     imports.push(imp);
                 }
             }
        }

        let data_tables = self.extract_data_tables(content);
        let service_calls = self.extract_service_calls(content);

        Ok(ParsedFile {
            path: path.to_string_lossy().to_string(),
            language: "go".to_string(),
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
    fn test_parse_go_full() {
        let parser = GoParser::new().unwrap();
        let content = r#"
            package main

            import (
                "fmt"
                "net/http"
            )

            type Server struct {
                port int
            }

            func (s *Server) Start() {
                fmt.Println("Starting server")
                http.ListenAndServe(":", nil)
            }

            func main() {
                server := Server{port: 8080}
                server.Start()
            }
        "#;
        
        let result = parser.parse_file(&PathBuf::from("test.go"), content).unwrap();
        
        // Imports
        assert!(result.imports.contains(&"fmt".to_string()));
        assert!(result.imports.contains(&"net/http".to_string()));
        
        // Structs
        let server = result.classes.iter().find(|c| c.name == "Server").expect("Server struct not found");
        assert!(server.methods.iter().any(|m| m.name == "Start"));
        
        // Functions
        let main = result.functions.iter().find(|f| f.name == "main").expect("main not found");
        assert!(main.calls.contains(&"Start".to_string()));
    }
}
