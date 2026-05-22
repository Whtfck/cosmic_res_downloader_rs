mod downloader;
mod models;
mod parser;
mod unzipper;
mod xml_parser;

use models::{FileItem, RunMode};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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

fn is_xml_file(task: &FileItem) -> bool {
    task.filename.ends_with(".xml")
}

fn is_webapp(task: &FileItem) -> bool {
    task.group == "webapp"
}

fn regex_filter(filename: &str, pattern: &str) -> bool {
    let re = regex::Regex::new(if pattern.is_empty() { r"\d+(?:bk|bak)|(?:bk|bak)\d+" } else { pattern }).ok();
    parser::should_filter(filename, re.as_ref())
}

fn emit_skip(app: &AppHandle, task: &FileItem) {
    let _ = app.emit(
        "download-progress",
        models::DownloadProgress {
            filename: task.filename.clone(),
            local_path: task.local_path.clone(),
            status: "skipped".to_string(),
            progress: 100,
            downloaded_bytes: 0,
            total_bytes: 0,
        },
    );
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
    xml_filter: bool,
    filter_pattern: String,
    ignored_files: Vec<String>,
    force_download_files: Vec<String>,
    app: AppHandle,
) -> Result<(), String> {
    downloader::reset_cancel();

    let ignored_set: std::collections::HashSet<String> = ignored_files.into_iter().collect();
    let force_set: std::collections::HashSet<String> = force_download_files.into_iter().collect();

    // Filter out ignored tasks, emit skipped for them
    let tasks: Vec<FileItem> = tasks
        .into_iter()
        .filter(|t| {
            if ignored_set.contains(&t.filename) {
                emit_skip(&app, t);
                false
            } else {
                true
            }
        })
        .collect();

    match mode {
        RunMode::DownloadOnly => {
            if xml_filter {
                download_with_xml_filter(tasks, &dest_dir, file_concurrency, chunk_threads, retries, &headers, &filter_pattern, &force_set, &app).await?;
            } else {
                downloader::start_download(tasks, dest_dir, file_concurrency, chunk_threads, retries, headers, force_set, app).await?;
            }
        }
        RunMode::UnzipOnly => {
            if xml_filter {
                let whitelist = xml_parser::collect_xml_whitelist(&dest_dir, &filter_pattern).await;
                unzipper::unzip_with_filters(&dest_dir, Some(&whitelist), &filter_pattern, file_concurrency, &app).await?;
            } else {
                unzipper::unzip_with_filters(&dest_dir, None, &filter_pattern, file_concurrency, &app).await?;
            }
        }
        RunMode::Both => {
            if xml_filter {
                download_with_xml_filter(tasks, &dest_dir, file_concurrency, chunk_threads, retries, &headers, &filter_pattern, &force_set, &app).await?;
                if downloader::is_cancelled() {
                    return Ok(());
                }
                let _ = app.emit("download-complete", ());
                let whitelist = xml_parser::collect_xml_whitelist(&dest_dir, &filter_pattern).await;
                unzipper::unzip_with_filters(&dest_dir, Some(&whitelist), &filter_pattern, file_concurrency, &app).await?;
            } else {
                downloader::start_download(
                    tasks,
                    dest_dir.clone(),
                    file_concurrency,
                    chunk_threads,
                    retries,
                    headers,
                    force_set,
                    app.clone(),
                )
                .await?;
                if downloader::is_cancelled() {
                    return Ok(());
                }
                let _ = app.emit("download-complete", ());
                unzipper::unzip_with_filters(&dest_dir, None, &filter_pattern, file_concurrency, &app).await?;
            }
        }
    }
    Ok(())
}

async fn download_with_xml_filter(
    tasks: Vec<FileItem>,
    dest_dir: &str,
    file_concurrency: usize,
    chunk_threads: usize,
    retries: usize,
    headers: &HashMap<String, String>,
    filter_pattern: &str,
    force_set: &HashSet<String>,
    app: &AppHandle,
) -> Result<(), String> {
    // Split by group: webapp vs appstore
    let (webapp_tasks, other_tasks): (Vec<_>, Vec<_>) = tasks
        .into_iter()
        .partition(|t| is_webapp(t));

    // Split appstore: XML files vs real files
    let (xml_tasks, appstore_tasks): (Vec<_>, Vec<_>) = other_tasks
        .into_iter()
        .partition(|t| is_xml_file(t));

    // --- webapp: only regex filter, no XML ---
    let mut webapp_filtered = Vec::new();
    for task in webapp_tasks {
        if downloader::is_cancelled() {
            emit_skip(app, &task);
            continue;
        }
        if !regex_filter(&task.filename, filter_pattern) {
            webapp_filtered.push(task);
        } else {
            eprintln!("webapp正则过滤跳过: {}", task.filename);
            emit_skip(app, &task);
        }
    }

    // --- appstore: download XML first, then filter by XML whitelist + regex ---
    // Step 1: Download XML files
    if !xml_tasks.is_empty() {
        downloader::start_download(
            xml_tasks,
            dest_dir.to_string(),
            file_concurrency,
            chunk_threads,
            retries,
            headers.clone(),
            HashSet::new(),
            app.clone(),
        )
        .await?;
        if downloader::is_cancelled() {
            return Ok(());
        }
    }

    // Step 2: Parse XML whitelist (already regex-filtered inside)
    let whitelist = xml_parser::collect_xml_whitelist(dest_dir, filter_pattern).await;
    eprintln!("XML白名单: {:?}", whitelist);

    // Step 3: Filter appstore files by whitelist
    let mut appstore_filtered = Vec::new();
    for task in appstore_tasks {
        if downloader::is_cancelled() {
            emit_skip(app, &task);
            continue;
        }
        if whitelist.contains(&task.filename) {
            appstore_filtered.push(task);
        } else {
            eprintln!("appstore XML过滤跳过: {}", task.filename);
            emit_skip(app, &task);
        }
    }

    // Step 4: Combine and download (with MD5 check)
    let mut all = webapp_filtered;
    all.extend(appstore_filtered);
    eprintln!("过滤后待下载: {} 个文件", all.len());

    if !all.is_empty() {
        downloader::start_download(
            all,
            dest_dir.to_string(),
            file_concurrency,
            chunk_threads,
            retries,
            headers.clone(),
            force_set.clone(),
            app.clone(),
        )
        .await?;
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
