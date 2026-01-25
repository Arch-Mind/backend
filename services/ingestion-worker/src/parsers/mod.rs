pub mod javascript;
pub mod typescript;

use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub path: String,
    pub language: String,
    pub functions: Vec<FunctionInfo>,
    #[allow(dead_code)]
    pub classes: Vec<ClassInfo>,
    pub imports: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub calls: Vec<String>,
    pub start_line: usize,
    pub end_line: usize,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub methods: Vec<String>,
}

pub trait LanguageParser {
    fn parse_file(&self, path: &PathBuf, content: &str) -> Result<ParsedFile>;
}
