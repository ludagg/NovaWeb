use crate::interpreter::Interpreter;
use crate::parser;
use crate::value::Value;
use anyhow::anyhow;
use axum::{
    body::Body,
    extract::{Path, Request as AxumRequest, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::services::ServeDir;

pub struct AppState {
    pub pages_dir: PathBuf,
    pub static_dir: PathBuf,
}

pub async fn serve(host: String, port: u16, pages_dir: PathBuf, static_dir: PathBuf) {
    let state = Arc::new(AppState {
        pages_dir,
        static_dir,
    });

    let app = Router::new()
        .route("/{*path}", get(handle_request).post(handle_request).put(handle_request).delete(handle_request))
        .nest_service("/static", ServeDir::new(state.static_dir.clone()))
        .with_state(state.clone());

    let addr = format!("{}:{}", host, port);
    println!("NovaWeb server listening on {}", addr);
    println!("Serving pages from: {}", state.pages_dir.display());
    println!("Serving static files from: {}", state.static_dir.display());

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_request(
    State(state): State<Arc<AppState>>,
    path: Path<String>,
    method: axum::http::Method,
    headers: HeaderMap,
    _uri: axum::http::Uri,
    query: axum::extract::Query<HashMap<String, String>>,
) -> Response {
    let request_path = path.0;

    // Build request context
    let mut request_map = HashMap::new();
    request_map.insert("method".to_string(), Value::String(method.to_string()));
    request_map.insert("path".to_string(), Value::String(request_path.clone()));

    // Add query parameters
    let mut query_map = HashMap::new();
    for (key, value) in query.0 {
        query_map.insert(key, Value::String(value));
    }
    request_map.insert("query".to_string(), Value::Map(query_map));

    // Add headers
    let mut headers_map = HashMap::new();
    for (key, value) in headers.iter() {
        if let Ok(value_str) = value.to_str() {
            headers_map.insert(key.as_str().to_string(), Value::String(value_str.to_string()));
        }
    }
    request_map.insert("headers".to_string(), Value::Map(headers_map));

    // Try to find a matching .novaw file
    let file_path = find_novaw_file(&state.pages_dir, &request_path);

    match file_path {
        Some(path) => match execute_novaw_file(&path, &request_map, &request_path) {
            Ok(result) => match result {
                Value::String(body) => (StatusCode::OK, body).into_response(),
                Value::Null => (StatusCode::NO_CONTENT, "").into_response(),
                Value::Int(n) => (StatusCode::OK, n.to_string()).into_response(),
                Value::Float(f) => (StatusCode::OK, f.to_string()).into_response(),
                Value::Bool(b) => (StatusCode::OK, b.to_string()).into_response(),
                Value::List(l) => {
                    let json = serde_json::to_string(&l).unwrap_or_default();
                    (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "application/json")], json).into_response()
                }
                Value::Map(m) => {
                    let json = serde_json::to_string(&m).unwrap_or_default();
                    (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "application/json")], json).into_response()
                }
                _ => (StatusCode::OK, result.to_string()).into_response(),
            },
            Err(e) => {
                eprintln!("Error executing {}: {}", path.display(), e);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e)).into_response()
            }
        },
        None => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

fn find_novaw_file(pages_dir: &PathBuf, request_path: &str) -> Option<PathBuf> {
    // Normalize the path
    let path = request_path.trim_start_matches('/');
    let path = if path.is_empty() { "index" } else { path };

    // First try exact match
    let exact_path = pages_dir.join(format!("{}.novaw", path));
    if exact_path.exists() {
        return Some(exact_path);
    }

    // Try to find dynamic segment match (e.g., [id].novaw)
    if let Some(file) = find_dynamic_file(pages_dir, path) {
        return Some(file);
    }

    None
}

fn find_dynamic_file(base_dir: &PathBuf, path: &str) -> Option<PathBuf> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if segments.is_empty() {
        // Check for index.novaw
        let index_path = base_dir.join("index.novaw");
        if index_path.exists() {
            return Some(index_path);
        }
        return None;
    }

    let mut current_path = base_dir.clone();

    for segment in segments {
        // First, try to find a regular directory/file
        let regular_path = current_path.join(segment);
        if regular_path.exists() {
            if regular_path.is_dir() {
                current_path = regular_path;
                continue;
            } else {
                // It's a file, check if it's the last segment and has .novaw extension
                if regular_path.extension().map_or(false, |e| e == "novaw") {
                    return Some(regular_path);
                }
                return None;
            }
        }

        // Try to find a dynamic segment [param]
        let mut found_dir = false;
        let _found_file = false;

        if let Ok(entries) = fs::read_dir(&current_path) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                // Check for dynamic directory [param]
                if name_str.starts_with('[') && name_str.ends_with(']') {
                    if entry.path().is_dir() {
                        current_path = entry.path();
                        found_dir = true;
                        break;
                    }
                }

                // Check for dynamic file [param].novaw
                if name_str.starts_with('[') && name_str.ends_with("].novaw") {
                    return Some(entry.path());
                }
            }
        }

        if found_dir {
            continue;
        }

        return None;
    }

    // If we've consumed all segments and are at a directory, check for index.novaw
    let index_path = current_path.join("index.novaw");
    if index_path.exists() {
        return Some(index_path);
    }

    None
}

fn execute_novaw_file(
    file_path: &PathBuf,
    request_context: &HashMap<String, Value>,
    request_path: &str,
) -> anyhow::Result<Value> {
    let content = fs::read_to_string(file_path)?;

    // Parse route parameters from the file path
    let route_params = extract_route_params(file_path, request_path);

    // Create a fresh interpreter for each request
    let mut interp = Interpreter::new();

    // Inject request object
    interp
        .globals
        .borrow_mut()
        .define("request".to_string(), Value::Map(request_context.clone()));

    // Inject route parameters
    if !route_params.is_empty() {
        interp
            .globals
            .borrow_mut()
            .define("params".to_string(), Value::Map(route_params));
    }

    // Parse and execute the script
    let statements = parser::parse(&content)?;
    interp.interpret(&statements)
}

fn extract_route_params(file_path: &PathBuf, request_path: &str) -> HashMap<String, Value> {
    let mut params = HashMap::new();

    let _file_str = file_path.to_string_lossy();
    let request_segments: Vec<&str> = request_path.split('/').filter(|s| !s.is_empty()).collect();

    // Find dynamic segments in the file path
    if let Some(parent) = file_path.parent() {
        let parent_str = parent.to_string_lossy();
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // Check if file name is dynamic [param].novaw
        if file_name.starts_with('[') && file_name.ends_with("].novaw") {
            let param_name = &file_name[1..file_name.len() - 6]; // Remove [ and ].novaw
            if let Some(last_segment) = request_segments.last() {
                params.insert(param_name.to_string(), Value::String(last_segment.to_string()));
            }
        }

        // Check parent directories for dynamic segments
        let parent_segments: Vec<&str> = parent_str
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        for (i, parent_seg) in parent_segments.iter().enumerate() {
            if parent_seg.starts_with('[') && parent_seg.ends_with(']') {
                let param_name = &parent_seg[1..parent_seg.len() - 1];
                if i < request_segments.len() {
                    params.insert(param_name.to_string(), Value::String(request_segments[i].to_string()));
                }
            }
        }
    }

    params
}
