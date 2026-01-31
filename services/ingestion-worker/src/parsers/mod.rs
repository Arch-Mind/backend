pub mod javascript;
pub mod typescript;
pub mod rust_parser;
pub mod go_parser;
pub mod python_parser;

use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub path: String,
    pub language: String,
    pub functions: Vec<FunctionInfo>,
    pub classes: Vec<ClassInfo>,
    pub imports: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: Option<String>,
    pub calls: Vec<String>,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub parents: Vec<String>,
    pub methods: Vec<FunctionInfo>,
    pub start_line: usize,
    pub end_line: usize,
}

pub trait LanguageParser {
    fn parse_file(&self, path: &PathBuf, content: &str) -> Result<ParsedFile>;
}
