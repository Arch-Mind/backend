use super::{ClassInfo, FunctionInfo, InheritanceInfo, LanguageParser, ParsedFile, ServiceCall};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashSet;
use std::path::PathBuf;
use tree_sitter::{Node, Parser, Query, QueryCursor};

pub struct PythonParser;

impl PythonParser {
    pub fn new() -> Result<Self> {
        Ok(PythonParser)
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
        // node is (parameters)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
             if child.kind() == "identifier" {
                 params.push(content[child.byte_range()].to_string());
             } else if child.kind() == "typed_parameter" || child.kind() == "default_parameter" || child.kind() == "typed_default_parameter"{
                 if let Some(name) = child.child_by_field_name("name") {
                      params.push(content[name.byte_range()].to_string());
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

impl LanguageParser for PythonParser {
    fn parse_file(&self, path: &PathBuf, content: &str) -> Result<ParsedFile> {
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_python::language())
            .context("Failed to set Python language")?;
        let tree = parser
            .parse(content, None)
            .context("Failed to parse Python file")?;

        let root_node = tree.root_node();
        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut imports = Vec::new();

        // queries
        let func_query = Query::new(
            tree_sitter_python::language(),
            r#"
            (function_definition
              name: (identifier) @func.name
              parameters: (parameters) @func.params
              body: (block) @func.body
            ) @func.def
            "#,
        )?;

        let class_query = Query::new(
             tree_sitter_python::language(),
             r#"
             (class_definition
               name: (identifier) @class.name
               body: (block) @class.body
             ) @class.def
             "#
        )?;
        
        let inheritance_query = Query::new(
             tree_sitter_python::language(),
             r#"
             (class_definition
                superclasses: (argument_list 
                    (identifier) @parent
                )
             )
             "#
        )?;

        let call_query = Query::new(
             tree_sitter_python::language(),
             r#"
             (call
               function: [
                 (identifier) @call.name
                 (attribute attribute: (identifier) @call.name)
               ]
             )
             "#
        )?;

        let import_query = Query::new(
            tree_sitter_python::language(),
            r#"
            (import_statement (dotted_name) @import.source)
            (import_from_statement module_name: (dotted_name) @import.source)
            (import_from_statement module_name: (relative_import) @import.source)
            "#,
        )?;

        let mut query_cursor = QueryCursor::new();
        
        let process_function = |node: Node, name: String| -> FunctionInfo {
             let start_line = node.start_position().row + 1;
             let end_line = node.end_position().row + 1;
             
             let mut params = Vec::new();
             if let Some(params_node) = node.child_by_field_name("parameters") {
                 params = self.extract_params(params_node, content);
             }
             let calls = self.extract_calls(node, content, &call_query);

             FunctionInfo {
                 name,
                 params,
                 return_type: None,
                 calls,
                 start_line,
                 end_line,
             }
        };

        // 1. Extract Classes
        let class_matches = query_cursor.matches(&class_query, root_node, content.as_bytes());
        for m in class_matches {
            let mut name = String::new();
            let mut node = root_node;
            let mut body_node = root_node;
            
            for c in m.captures {
                let cn = &class_query.capture_names()[c.index as usize];
                if cn == "class.name" {
                    name = content[c.node.byte_range()].to_string();
                } else if cn == "class.def" {
                    node = c.node;
                } else if cn == "class.body" {
                    body_node = c.node;
                }
            }

            if !name.is_empty() {
                 let start_line = node.start_position().row + 1;
                 let end_line = node.end_position().row + 1;
                 
                 let mut inheritances = Vec::new();
                 let mut parent_cursor = QueryCursor::new();
                 let im = parent_cursor.matches(&inheritance_query, node, content.as_bytes());
                 for pm in im {
                     for c in pm.captures {
                         if inheritance_query.capture_names()[c.index as usize] == "parent" {
                              inheritances.push(InheritanceInfo {
                                  name: content[c.node.byte_range()].to_string(),
                                  kind: "class".to_string(),
                              });
                         }
                     }
                 }

                 let mut methods = Vec::new();
                 let mut method_cursor = QueryCursor::new();
                 let mm = method_cursor.matches(&func_query, body_node, content.as_bytes());
                 for f in mm {
                      let mut m_name = String::new();
                      let mut m_node = root_node;
                      for c in f.captures {
                          if func_query.capture_names()[c.index as usize] == "func.name" {
                              m_name = content[c.node.byte_range()].to_string();
                          } else if func_query.capture_names()[c.index as usize] == "func.def" {
                              m_node = c.node;
                          }
                      }
                      if !m_name.is_empty() {
                           methods.push(process_function(m_node, m_name));
                      }
                 }
                 
                 classes.push(ClassInfo {
                     name,
                     inheritances,
                     methods,
                     start_line,
                     end_line,
                 });
            }
        }

        // 2. Extract Top Level Functions
        let func_matches = query_cursor.matches(&func_query, root_node, content.as_bytes());
        for m in func_matches {
            let mut name = String::new();
            let mut node = root_node;
            for c in m.captures {
                if func_query.capture_names()[c.index as usize] == "func.name" {
                    name = content[c.node.byte_range()].to_string();
                } else if func_query.capture_names()[c.index as usize] == "func.def" {
                    node = c.node;
                }
            }
            
            if !name.is_empty() {
                let mut is_method = false;
                if let Some(parent) = node.parent() { 
                     if let Some(grandparent) = parent.parent() {
                         if grandparent.kind() == "class_definition" {
                             is_method = true;
                         }
                     }
                }
                
                if !is_method {
                    functions.push(process_function(node, name));
                }
            }
        }

        // 3. Imports
        let imports_matches = query_cursor.matches(&import_query, root_node, content.as_bytes());
        for m in imports_matches {
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
            language: "python".to_string(),
            functions,
            classes,
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
    fn test_parse_python_full() {
        let parser = PythonParser::new().unwrap();
        let content = r#"
            import os
            from typing import List

            class Processor(BaseProcessor):
                def process(self, data: List[str]):
                    self.clean(data)
                    return data

                def clean(self, data):
                    pass

            def main():
                p = Processor()
                p.process(["foo"])
        "#;
        
        let result = parser.parse_file(&PathBuf::from("test.py"), content).unwrap();
        
        // Imports
        assert!(result.imports.contains(&"os".to_string()));
        assert!(result.imports.contains(&"typing".to_string()));
        
        // Classes
        let processor = result.classes.iter().find(|c| c.name == "Processor").expect("Processor not found");
        assert!(processor
            .inheritances
            .iter()
            .any(|inheritance| inheritance.name == "BaseProcessor" && inheritance.kind == "class"));
        
        let process = processor.methods.iter().find(|m| m.name == "process").expect("process not found");
        assert_eq!(process.params, vec!["self", "data"]);
        assert!(process.calls.contains(&"clean".to_string())); // self.clean -> clean
        
        // Functions
        let main = result.functions.iter().find(|f| f.name == "main").expect("main not found");
        assert!(main.calls.contains(&"process".to_string()));
    }
}
