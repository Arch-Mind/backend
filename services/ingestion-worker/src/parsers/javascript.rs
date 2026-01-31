use super::{ClassInfo, FunctionInfo, LanguageParser, ParsedFile};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::PathBuf;
use tree_sitter::{Node, Parser, Query, QueryCursor};

pub struct JavaScriptParser;

impl JavaScriptParser {
    pub fn new() -> Result<Self> {
        Ok(JavaScriptParser)
    }

    fn extract_params(&self, node: Node, content: &str) -> Vec<String> {
        let mut params = Vec::new();
        let mut cursor = node.walk();
        
        for child in node.children(&mut cursor) {
            if child.kind() == "formal_parameters" {
                let mut param_cursor = child.walk();
                for param in child.children(&mut param_cursor) {
                    if param.kind() == "identifier" {
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

impl LanguageParser for JavaScriptParser {
    fn parse_file(&self, path: &PathBuf, content: &str) -> Result<ParsedFile> {
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_javascript::language())
            .context("Failed to set JavaScript language")?;
        let tree = parser
            .parse(content, None)
            .context("Failed to parse JavaScript file")?;

        let root_node = tree.root_node();
        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut imports = Vec::new();

        // Queries
        let function_query = Query::new(
            tree_sitter_javascript::language(),
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
            tree_sitter_javascript::language(),
            r#"
            (class_declaration
                name: (identifier) @class.name
                body: (class_body) @class.body
            ) @class.def
            "#,
        ).context("Failed to create class query")?;
        
        let inheritance_query = Query::new(
             tree_sitter_javascript::language(),
             r#"
             (class_heritage (identifier) @parent)
             "#
        ).context("Failed to create inheritance query")?;

        let call_query = Query::new(
            tree_sitter_javascript::language(),
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
            tree_sitter_javascript::language(),
            r#"
            (import_statement
              source: (string) @import.source)
            
            (call_expression
              function: (identifier) @import.func
              (#eq? @import.func "require")
              arguments: (arguments
                (string) @import.source))
            "#,
        ).context("Failed to create import query")?;

        let mut query_cursor = QueryCursor::new();

        let process_function = |node: Node, name: String, _params_node: Option<Node>| -> FunctionInfo {
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

        // Extract Top-Level Functions
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
                 functions.push(process_function(func_node, func_name, None));
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

                 let mut parents = Vec::new();
                 let parent_matches = query_cursor.matches(&inheritance_query, class_node, content.as_bytes());
                 for pm in parent_matches {
                      for c in pm.captures {
                          parents.push(content[c.node.byte_range()].to_string());
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
                         methods.push(process_function(method_node, method_name, None));
                     }
                 }

                 classes.push(ClassInfo {
                     name: class_name,
                     parents,
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

        Ok(ParsedFile {
            path: path.to_string_lossy().to_string(),
            language: "javascript".to_string(),
            functions,
            classes,
            imports,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_js_full() {
        let parser = JavaScriptParser::new().unwrap();
        let content = r#"
            import axios from 'axios';
            const log = require('logger');
            
            function add(a, b = 1) {
                return a + b;
            }
            
            const subtract = (a, b) => a - b;
            
            class Calculator extends BaseCalc {
                constructor() { super(); }
                
                multiply(a, b) {
                    this.log(a * b);
                    return a * b;
                }
            }
            
            add(1, 2);
        "#;
        
        let result = parser.parse_file(&PathBuf::from("test.js"), content).unwrap();
        
        // Imports
        assert!(result.imports.contains(&"axios".to_string()));
        assert!(result.imports.contains(&"logger".to_string()));
        
        // Functions
        let add_fn = result.functions.iter().find(|f| f.name == "add").expect("add not found");
        assert_eq!(add_fn.params, vec!["a", "b"]);
        
        // Classes
        let calc_class = result.classes.iter().find(|c| c.name == "Calculator").expect("Calculator not found");
        assert!(calc_class.parents.contains(&"BaseCalc".to_string()));
        
        let mult_method = calc_class.methods.iter().find(|m| m.name == "multiply").expect("multiply not found");
        assert_eq!(mult_method.params, vec!["a", "b"]);
        assert!(mult_method.calls.iter().any(|c| c == "log")); // this.log -> log in simplified extract
    }
}
