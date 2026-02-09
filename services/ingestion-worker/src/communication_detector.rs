use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

use crate::parsers::ParsedFile;

#[derive(Debug, Clone)]
pub struct EndpointCall {
    pub file_path: String,
    pub url: String,
    pub method: String,
    pub host: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RpcCall {
    pub file_path: String,
    pub service_name: String,
}

#[derive(Debug, Clone)]
pub enum QueueDirection {
    Publish,
    Consume,
}

#[derive(Debug, Clone)]
pub struct QueueUsage {
    pub file_path: String,
    pub topic: String,
    pub direction: QueueDirection,
}

#[derive(Debug, Clone)]
pub struct ComposeService {
    pub name: String,
    pub ports: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CommunicationAnalysis {
    pub endpoints: Vec<EndpointCall>,
    pub rpc_services: Vec<RpcCall>,
    pub queues: Vec<QueueUsage>,
    pub compose_services: Vec<ComposeService>,
}

pub struct CommunicationDetector;

impl CommunicationDetector {
    pub fn detect(repo_path: &PathBuf, parsed_files: &[ParsedFile]) -> Result<CommunicationAnalysis> {
        let mut endpoints = Vec::new();
        let mut rpc_services = Vec::new();
        let mut queues = Vec::new();

        for file in parsed_files {
            let file_path = repo_path.join(Path::new(&file.path));
            let content = match fs::read_to_string(&file_path) {
                Ok(data) => data,
                Err(_) => continue,
            };

            endpoints.extend(extract_http_calls(&file.path, &content));
            rpc_services.extend(extract_grpc_calls(&file.path, &content));
            queues.extend(extract_queue_calls(&file.path, &content));
        }

        let proto_services = extract_proto_services(repo_path)?;
        for svc in proto_services {
            rpc_services.push(RpcCall {
                file_path: "proto".to_string(),
                service_name: svc,
            });
        }

        let compose_services = parse_docker_compose(repo_path)?;

        Ok(CommunicationAnalysis {
            endpoints,
            rpc_services,
            queues,
            compose_services,
        })
    }
}

fn extract_http_calls(file_path: &str, content: &str) -> Vec<EndpointCall> {
    let mut calls = Vec::new();

    let fetch_re = Regex::new(r#"(?i)fetch\(\s*['\"](https?://[^'\"\s]+)['\"]"#).ok();
    let fetch_method_re = Regex::new(r#"(?i)fetch\(\s*['\"](https?://[^'\"\s]+)['\"]\s*,\s*\{{[^}}]*method\s*:\s*['\"]([A-Z]+)['\"]"#).ok();
    let axios_re = Regex::new(r#"(?i)axios\.(get|post|put|delete|patch)\(\s*['\"](https?://[^'\"\s]+)['\"]"#).ok();
    let requests_re = Regex::new(r#"(?i)requests\.(get|post|put|delete|patch)\(\s*['\"](https?://[^'\"\s]+)['\"]"#).ok();
    let http_get_re = Regex::new(r#"(?i)http\.Get\(\s*\"(https?://[^\"\s]+)\""#).ok();

    if let Some(re) = fetch_method_re.as_ref() {
        for cap in re.captures_iter(content) {
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or_default().to_string();
            let method = cap.get(2).map(|m| m.as_str()).unwrap_or("GET").to_string();
            calls.push(make_endpoint_call(file_path, url, method));
        }
    }

    if let Some(re) = fetch_re.as_ref() {
        for cap in re.captures_iter(content) {
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or_default().to_string();
            calls.push(make_endpoint_call(file_path, url, "GET".to_string()));
        }
    }

    if let Some(re) = axios_re.as_ref() {
        for cap in re.captures_iter(content) {
            let method = cap.get(1).map(|m| m.as_str()).unwrap_or("get").to_uppercase();
            let url = cap.get(2).map(|m| m.as_str()).unwrap_or_default().to_string();
            calls.push(make_endpoint_call(file_path, url, method));
        }
    }

    if let Some(re) = requests_re.as_ref() {
        for cap in re.captures_iter(content) {
            let method = cap.get(1).map(|m| m.as_str()).unwrap_or("get").to_uppercase();
            let url = cap.get(2).map(|m| m.as_str()).unwrap_or_default().to_string();
            calls.push(make_endpoint_call(file_path, url, method));
        }
    }

    if let Some(re) = http_get_re.as_ref() {
        for cap in re.captures_iter(content) {
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or_default().to_string();
            calls.push(make_endpoint_call(file_path, url, "GET".to_string()));
        }
    }

    calls
}

fn make_endpoint_call(file_path: &str, url: String, method: String) -> EndpointCall {
    let host = extract_host(&url);
    EndpointCall {
        file_path: file_path.to_string(),
        url,
        method,
        host,
    }
}

fn extract_grpc_calls(file_path: &str, content: &str) -> Vec<RpcCall> {
    let mut calls = Vec::new();
    let dial_re = Regex::new(r#"(?i)grpc\.Dial\(\s*\"([^\"]+)\""#).ok();
    let grpc_client_re = Regex::new(r#"(?i)@grpc/grpc-js"#).ok();

    if let Some(re) = dial_re.as_ref() {
        for cap in re.captures_iter(content) {
            let target = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
            calls.push(RpcCall {
                file_path: file_path.to_string(),
                service_name: target.to_string(),
            });
        }
    }

    if grpc_client_re
        .as_ref()
        .map(|re| re.is_match(content))
        .unwrap_or(false)
    {
        calls.push(RpcCall {
            file_path: file_path.to_string(),
            service_name: "grpc-js".to_string(),
        });
    }

    calls
}

fn extract_queue_calls(file_path: &str, content: &str) -> Vec<QueueUsage> {
    let mut queues = Vec::new();

    let publish_re = Regex::new(r#"(?i)(producer\.send|kafka\.publish|channel\.publish)\([^\)]*['\"]([A-Za-z0-9_.-]+)['\"]"#).ok();
    let subscribe_re = Regex::new(r#"(?i)(consumer\.subscribe|kafka\.subscribe)\([^\)]*['\"]([A-Za-z0-9_.-]+)['\"]"#).ok();

    if let Some(re) = publish_re.as_ref() {
        for cap in re.captures_iter(content) {
            let topic = cap.get(2).map(|m| m.as_str()).unwrap_or_default().to_string();
            queues.push(QueueUsage {
                file_path: file_path.to_string(),
                topic,
                direction: QueueDirection::Publish,
            });
        }
    }

    if let Some(re) = subscribe_re.as_ref() {
        for cap in re.captures_iter(content) {
            let topic = cap.get(2).map(|m| m.as_str()).unwrap_or_default().to_string();
            queues.push(QueueUsage {
                file_path: file_path.to_string(),
                topic,
                direction: QueueDirection::Consume,
            });
        }
    }

    queues
}

fn extract_proto_services(repo_path: &PathBuf) -> Result<Vec<String>> {
    let mut services = Vec::new();
    let mut proto_files = Vec::new();
    collect_proto_files(repo_path, &mut proto_files)?;

    let service_re = Regex::new(r"(?i)\bservice\s+([A-Za-z0-9_]+)").context("Failed to build proto service regex")?;

    for file in proto_files {
        if let Ok(content) = fs::read_to_string(&file) {
            for cap in service_re.captures_iter(&content) {
                if let Some(m) = cap.get(1) {
                    services.push(m.as_str().to_string());
                }
            }
        }
    }

    Ok(services)
}

fn collect_proto_files(current_dir: &PathBuf, results: &mut Vec<PathBuf>) -> Result<()> {
    if !current_dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(current_dir).context("Failed to read directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if let Some(name) = path.file_name() {
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.')
                || name_str == "node_modules"
                || name_str == "target"
                || name_str == "dist"
                || name_str == "build"
                || name_str == "venv"
                || name_str == "__pycache__" {
                continue;
            }
        }

        if path.is_dir() {
            collect_proto_files(&path, results)?;
        } else if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext.to_string_lossy().to_lowercase() == "proto" {
                    results.push(path);
                }
            }
        }
    }

    Ok(())
}

fn parse_docker_compose(repo_path: &PathBuf) -> Result<Vec<ComposeService>> {
    let compose_path = repo_path.join("docker-compose.yml");
    if !compose_path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&compose_path).context("Failed to read docker-compose.yml")?;
    let mut services = Vec::new();

    let mut in_services = false;
    let mut in_ports = false;
    let mut current_service: Option<ComposeService> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        let indent = line.chars().take_while(|c| *c == ' ').count();
        if trimmed.starts_with("services:") {
            in_services = true;
            continue;
        }

        if !in_services {
            continue;
        }

        if indent == 2 && trimmed.ends_with(':') && !trimmed.starts_with('-') {
            if let Some(service) = current_service.take() {
                services.push(service);
            }
            let name = trimmed.trim_end_matches(':').to_string();
            current_service = Some(ComposeService { name, ports: Vec::new() });
            in_ports = false;
            continue;
        }

        if indent >= 4 && trimmed.starts_with("ports:") {
            in_ports = true;
            continue;
        }

        if in_ports && indent >= 6 && trimmed.starts_with('-') {
            if let Some(service) = current_service.as_mut() {
                let port = trimmed.trim_start_matches('-').trim().trim_matches('"').to_string();
                if !port.is_empty() {
                    service.ports.push(port);
                }
            }
        }
    }

    if let Some(service) = current_service {
        services.push(service);
    }

    Ok(services)
}

fn extract_host(url: &str) -> Option<String> {
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
