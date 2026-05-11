mod downloader;
mod models;
mod parser;
mod unzipper;

use models::{FileItem, RunMode};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};

#[derive(Serialize)]
struct FetchResult {
    json: Value,
    base_url: String,
}

fn ensure_update_json(url: &str) -> String {
    if url.ends_with("update.json") {
        url.to_string()
    } else if url.ends_with('/') {
        format!("{}update.json", url)
    } else {
        format!("{}/update.json", url)
    }
}

fn derive_base_url(url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    let base = format!(
        "{}://{}",
        parsed.scheme(),
        if let Some(port) = parsed.port() {
            format!("{}:{}", parsed.host_str().unwrap_or(""), port)
        } else {
            parsed.host_str().unwrap_or("").to_string()
        }
    );
    Ok(base)
}

fn build_headers(headers: &HashMap<String, String>) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    for (k, v) in headers {
        if let (Ok(name), Ok(val)) = (
            reqwest::header::HeaderName::from_bytes(k.as_bytes()),
            reqwest::header::HeaderValue::from_str(v),
        ) {
            map.insert(name, val);
        }
    }
    map
}

#[tauri::command]
async fn fetch_json(
    url: String,
    headers: HashMap<String, String>,
) -> Result<FetchResult, String> {
    let full_url = ensure_update_json(&url);
    let base_url = derive_base_url(&full_url)?;

    let client = reqwest::Client::new();
    let mut req = client.get(&full_url);
    if !headers.is_empty() {
        req = req.headers(build_headers(&headers));
    }

    let response = req.send().await.map_err(|e| format!("获取JSON失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("获取JSON失败: HTTP {}", response.status()));
    }

    let json: Value = response
        .json()
        .await
        .map_err(|e| format!("解析JSON失败: {}", e))?;

    Ok(FetchResult { json, base_url })
}

#[tauri::command]
async fn parse_file_list(
    json: Value,
    base_url: String,
    filter_pattern: String,
) -> Result<Vec<FileItem>, String> {
    Ok(parser::parse_file_list(&json, &base_url, &filter_pattern))
}

#[tauri::command]
async fn start_process(
    tasks: Vec<FileItem>,
    dest_dir: String,
    file_concurrency: usize,
    chunk_threads: usize,
    retries: usize,
    mode: RunMode,
    headers: HashMap<String, String>,
    app: AppHandle,
) -> Result<(), String> {
    downloader::reset_cancel();

    match mode {
        RunMode::DownloadOnly => {
            downloader::start_download(tasks, dest_dir, file_concurrency, chunk_threads, retries, headers, app).await?;
        }
        RunMode::UnzipOnly => {
            unzipper::unzip_all(&dest_dir, &app).await?;
        }
        RunMode::Both => {
            downloader::start_download(
                tasks,
                dest_dir.clone(),
                file_concurrency,
                chunk_threads,
                retries,
                headers,
                app.clone(),
            )
            .await?;
            if downloader::is_cancelled() {
                return Ok(());
            }
            // Signal that download phase is complete
            let _ = app.emit("download-complete", ());
            unzipper::unzip_all(&dest_dir, &app).await?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn cancel_process() {
    downloader::request_cancel();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            fetch_json,
            parse_file_list,
            start_process,
            cancel_process
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
