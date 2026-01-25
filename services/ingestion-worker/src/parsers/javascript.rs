use super::{FunctionInfo, LanguageParser, ParsedFile};
use anyhow::{Context, Result};
use std::path::PathBuf;
use tree_sitter::{Parser, Query, QueryCursor};

pub struct JavaScriptParser;

impl JavaScriptParser {
    pub fn new() -> Result<Self> {
        Ok(JavaScriptParser)
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
        let mut imports = Vec::new();

        // Query for function declarations and expressions
        let function_query = Query::new(
            tree_sitter_javascript::language(),
            r#"
            (function_declaration
              name: (identifier) @func.name) @func.def
            
            (variable_declarator
              name: (identifier) @func.name
              value: [
                (function_expression)
                (arrow_function)
              ]) @func.def
            
            (method_definition
              name: (property_identifier) @func.name) @func.def
            "#,
        )
        .context("Failed to create function query")?;

        // Query for function calls
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
        )
        .context("Failed to create call query")?;

        // Query for imports
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
        )
        .context("Failed to create import query")?;

        let mut query_cursor = QueryCursor::new();

        // Extract function definitions
        let func_matches = query_cursor.matches(&function_query, root_node, content.as_bytes());
        for func_match in func_matches {
            let mut func_name = String::new();
            let mut start_line = 0;
            let mut end_line = 0;

            for capture in func_match.captures {
                let capture_name = &function_query.capture_names()[capture.index as usize];
                if capture_name == "func.name" {
                    func_name = content[capture.node.byte_range()].to_string();
                } else if capture_name == "func.def" {
                    start_line = capture.node.start_position().row + 1;
                    end_line = capture.node.end_position().row + 1;
                }
            }

            // Find function calls within this function
            let mut calls = Vec::new();
            if !func_name.is_empty() {
                // Re-parse to find calls within this specific function node
                // For now, we'll collect all calls in the file (simplified)
                let mut call_cursor = QueryCursor::new();
                let call_matches = call_cursor.matches(&call_query, root_node, content.as_bytes());
                for call_match in call_matches {
                    for capture in call_match.captures {
                        let capture_name = &call_query.capture_names()[capture.index as usize];
                        if capture_name == "call.name" {
                            let call_name = content[capture.node.byte_range()].to_string();
                            if !calls.contains(&call_name) {
                                calls.push(call_name);
                            }
                        }
                    }
                }
            }

            if !func_name.is_empty() {
                functions.push(FunctionInfo {
                    name: func_name,
                    calls,
                    start_line,
                    end_line,
                });
            }
        }

        // Extract imports
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
            classes: Vec::new(), // TODO: Implement class extraction
            imports,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_javascript_functions() {
        let parser = JavaScriptParser::new().unwrap();
        let content = r#"
            import axios from 'axios';
            const fetch = require('node-fetch');

            function greet(name) {
                console.log('Hello ' + name);
                return formatMessage(name);
            }

            const add = (a, b) => {
                return a + b;
            };

            class MyClass {
                myMethod() {
                    greet('World');
                }
            }
        "#;

        let result = parser.parse_file(&PathBuf::from("test.js"), content).unwrap();

        assert!(result.functions.iter().any(|f| f.name == "greet"));
        assert!(result.functions.iter().any(|f| f.name == "add"));
        assert!(result.imports.contains(&"axios".to_string()));
        assert!(result.imports.contains(&"node-fetch".to_string()));
    }
}
