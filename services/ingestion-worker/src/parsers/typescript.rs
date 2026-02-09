use super::{ClassInfo, FunctionInfo, LanguageParser, ParsedFile};
use super::{InheritanceInfo, ServiceCall};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashSet;
use std::path::PathBuf;
use tree_sitter::{Node, Parser, Query, QueryCursor};

pub struct TypeScriptParser;

impl TypeScriptParser {
    pub fn new() -> Result<Self> {
        Ok(TypeScriptParser)
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
            if child.kind() == "formal_parameters" {
                let mut param_cursor = child.walk();
                for param in child.children(&mut param_cursor) {
                    if param.kind() == "required_parameter" || param.kind() == "optional_parameter" {
                        if let Some(pattern) = param.child_by_field_name("pattern") {
                             if pattern.kind() == "identifier" {
                                 params.push(content[pattern.byte_range()].to_string());
                             }
                        }
                    } else if param.kind() == "identifier" { 
                         params.push(content[param.byte_range()].to_string());
                    } else if param.kind() == "assignment_pattern" {
                         if let Some(left) = param.child_by_field_name("left") {
                             params.push(content[left.byte_range()].to_string());
                         }
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

impl LanguageParser for TypeScriptParser {
    fn parse_file(&self, path: &PathBuf, content: &str) -> Result<ParsedFile> {
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_typescript::language_typescript())
            .context("Failed to set TypeScript language")?;
        let tree = parser
            .parse(content, None)
            .context("Failed to parse TypeScript file")?;

        let root_node = tree.root_node();
        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut imports = Vec::new();

        // queries
        let function_query = Query::new(
            tree_sitter_typescript::language_typescript(),
            r#"
            (function_declaration
              name: (identifier) @func.name
              parameters: (formal_parameters) @func.params
              body: (statement_block) @func.body
            ) @func.def
            
            (variable_declarator
              name: (identifier) @func.name
              value: [
                (function_expression
                    parameters: (formal_parameters) @func.params
                    body: (statement_block) @func.body)
                (arrow_function
                    parameters: (formal_parameters) @func.params
                    body: [ (statement_block) (expression) ] @func.body)
              ]
            ) @func.def

            (method_definition
                name: (property_identifier) @func.name
                parameters: (formal_parameters) @func.params
                body: (statement_block) @func.body
            ) @func.def
            "#,
        ).context("Failed to create function query")?;

        let class_query = Query::new(
            tree_sitter_typescript::language_typescript(),
            r#"
            (class_declaration
                name: (type_identifier) @class.name
                body: (class_body) @class.body
            ) @class.def
            "#,
        ).context("Failed to create class query")?;
        
        let inheritance_query = Query::new(
            tree_sitter_typescript::language_typescript(),
             r#"
             (class_heritage
                (extends_clause value: (identifier) @parent.extends)
                (implements_clause (type_identifier) @parent.implements)
             )
             "#
        ).context("Failed to create inheritance query")?;

        let call_query = Query::new(
            tree_sitter_typescript::language_typescript(),
            r#"
            (call_expression
              function: [
                (identifier) @call.name
                (member_expression
                  property: (property_identifier) @call.name)
              ])
            "#,
        ).context("Failed to create call query")?;

        let import_query = Query::new(
            tree_sitter_typescript::language_typescript(),
            r#"
            (import_statement
              source: (string) @import.source)
            "#,
        ).context("Failed to create import query")?;
        
        let mut query_cursor = QueryCursor::new();

        let process_function = |node: Node, name: String| -> FunctionInfo {
             let start_line = node.start_position().row + 1;
             let end_line = node.end_position().row + 1;
             
             let params = self.extract_params(node, content); 
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

        // Extract Functions
        let func_matches = query_cursor.matches(&function_query, root_node, content.as_bytes());
        for func_match in func_matches {
            let mut func_name = String::new();
            let mut func_node = root_node;
            
            for capture in func_match.captures {
                let capture_name = &function_query.capture_names()[capture.index as usize];
                if capture_name == "func.name" {
                    func_name = content[capture.node.byte_range()].to_string();
                } else if capture_name == "func.def" {
                    func_node = capture.node;
                }
            }
            
            if func_node.kind() == "method_definition" {
                continue; 
            }

            if !func_name.is_empty() {
                 functions.push(process_function(func_node, func_name));
            }
        }

        // Extract Classes
        let class_matches = query_cursor.matches(&class_query, root_node, content.as_bytes());
        for class_match in class_matches {
             let mut class_name = String::new();
             let mut class_node = root_node;
             let mut body_node = root_node;

             for capture in class_match.captures {
                 let capture_name = &class_query.capture_names()[capture.index as usize];
                 if capture_name == "class.name" {
                     class_name = content[capture.node.byte_range()].to_string();
                 } else if capture_name == "class.def" {
                     class_node = capture.node;
                 } else if capture_name == "class.body" {
                     body_node = capture.node;
                 }
             }

             if !class_name.is_empty() {
                 let start_line = class_node.start_position().row + 1;
                 let end_line = class_node.end_position().row + 1;

                 let mut inheritances = Vec::new();
                 let mut parent_cursor = QueryCursor::new();
                 let parent_matches = parent_cursor.matches(&inheritance_query, class_node, content.as_bytes());
                 for pm in parent_matches {
                      for c in pm.captures {
                          let capture_name = &inheritance_query.capture_names()[c.index as usize];
                          if capture_name == "parent.extends" {
                              inheritances.push(InheritanceInfo {
                                  name: content[c.node.byte_range()].to_string(),
                                  kind: "class".to_string(),
                              });
                          } else if capture_name == "parent.implements" {
                              inheritances.push(InheritanceInfo {
                                  name: content[c.node.byte_range()].to_string(),
                                  kind: "interface".to_string(),
                              });
                          }
                      }
                 }

                 let mut methods = Vec::new();
                 let mut method_cursor = QueryCursor::new();
                 let method_matches = method_cursor.matches(&function_query, body_node, content.as_bytes());
                 
                 for mm in method_matches {
                     let mut method_name = String::new();
                     let mut method_node = root_node;
                     
                     for capture in mm.captures {
                        let capture_name = &function_query.capture_names()[capture.index as usize];
                        if capture_name == "func.name" {
                            method_name = content[capture.node.byte_range()].to_string();
                        } else if capture_name == "func.def" {
                            method_node = capture.node;
                        }
                     }
                     
                     if method_node.kind() == "method_definition" {
                         methods.push(process_function(method_node, method_name));
                     }
                 }

                 classes.push(ClassInfo {
                     name: class_name,
                     inheritances,
                     methods,
                     start_line,
                     end_line,
                 });
             }
        }

        // Extract Imports
        let import_matches = query_cursor.matches(&import_query, root_node, content.as_bytes());
        for import_match in import_matches {
            for capture in import_match.captures {
                let capture_name = &import_query.capture_names()[capture.index as usize];
                if capture_name == "import.source" {
                    let import_source = content[capture.node.byte_range()]
                        .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                        .to_string();
                    if !imports.contains(&import_source) {
                        imports.push(import_source);
                    }
                }
            }
        }

        let data_tables = self.extract_data_tables(content);
        let service_calls = self.extract_service_calls(content);

        Ok(ParsedFile {
            path: path.to_string_lossy().to_string(),
            language: "typescript".to_string(),
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
    fn test_parse_ts_full() {
        let parser = TypeScriptParser::new().unwrap();
        let content = r#"
            import { Foo } from 'bar';
            
            function process(data: string, options?: any) {
                return validate(data);
            }
            
            class User extends Person {
                update(id: number, name: string) {
                    this.save(id, name);
                }
            }
        "#;
        
        let result = parser.parse_file(&PathBuf::from("test.ts"), content).unwrap();
        
        // Imports
        assert!(result.imports.contains(&"bar".to_string()));
        
        // Functions
        let proc = result.functions.iter().find(|f| f.name == "process").expect("process not found");
        assert_eq!(proc.params, vec!["data", "options"]);
        assert!(proc.calls.contains(&"validate".to_string()));
        
        // Classes
        let user = result.classes.iter().find(|c| c.name == "User").expect("User not found");
        assert!(user
            .inheritances
            .iter()
            .any(|inheritance| inheritance.name == "Person" && inheritance.kind == "class"));
        
        let update = user.methods.iter().find(|m| m.name == "update").expect("update not found");
        assert_eq!(update.params, vec!["id", "name"]);
    }
}
