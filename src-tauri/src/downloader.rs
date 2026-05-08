use crate::models::{DownloadProgress, FileItem};
use futures_util::StreamExt;
use md5::{Digest, Md5};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;

const CHUNK_SIZE: u64 = 64 * 1024;

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

static CANCEL_FLAG: AtomicBool = AtomicBool::new(false);
static ALL_DONE: AtomicBool = AtomicBool::new(false);
static TOTAL_BYTES: AtomicU64 = AtomicU64::new(0);

pub fn request_cancel() {
    CANCEL_FLAG.store(true, Ordering::Relaxed);
}

pub fn reset_cancel() {
    CANCEL_FLAG.store(false, Ordering::Relaxed);
    ALL_DONE.store(false, Ordering::Relaxed);
    TOTAL_BYTES.store(0, Ordering::Relaxed);
}

pub fn is_cancelled() -> bool {
    CANCEL_FLAG.load(Ordering::Relaxed)
}

pub async fn clean_tmp_files(dest_dir: &str) {
    let dir = Path::new(dest_dir);
    if !dir.exists() {
        return;
    }
    clean_tmp_recursive(dir).await;
}

async fn clean_tmp_recursive(dir: &Path) {
    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_dir() {
            Box::pin(clean_tmp_recursive(&path)).await;
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.contains(".tmp.") || (name.contains(".part") && name.rsplit_once(".part").map_or(false, |(_, suffix)| suffix.chars().all(|c| c.is_ascii_digit()))) {
                let _ = fs::remove_file(&path).await;
            }
        }
    }
}

/// Spawn a task that emits speed (bytes/sec) every second
fn spawn_speed_emitter(app: AppHandle) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_bytes = TOTAL_BYTES.load(Ordering::Relaxed);
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let current = TOTAL_BYTES.load(Ordering::Relaxed);
            let speed = current.saturating_sub(last_bytes);
            last_bytes = current;
            let _ = app.emit("download-speed", speed);
            if is_cancelled() || ALL_DONE.load(Ordering::Relaxed) {
                break;
            }
        }
    })
}

pub async fn start_download(
    tasks: Vec<FileItem>,
    dest_dir: String,
    file_concurrency: usize,
    chunk_threads: usize,
    retries: usize,
    headers: HashMap<String, String>,
    app: AppHandle,
) -> Result<(), String> {
    reset_cancel();
    clean_tmp_files(&dest_dir).await;

    let semaphore = Arc::new(Semaphore::new(file_concurrency));
    let header_map = build_headers(&headers);
    let client = reqwest::Client::builder()
        .default_headers(header_map)
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(60))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {}", e))?;
    let dest_dir = Arc::new(dest_dir);

    let speed_handle = spawn_speed_emitter(app.clone());

    let mut handles = Vec::new();
    let total = tasks.len();
    let completed_count = Arc::new(AtomicU64::new(0));

    for (idx, task) in tasks.into_iter().enumerate() {
        if is_cancelled() {
            emit_progress(&app, &task.filename, &task.local_path, "pending", 0, 0, 0);
            continue;
        }

        let sem = semaphore.clone();
        let client = client.clone();
        let app = app.clone();
        let dest = dest_dir.clone();
        let counter = completed_count.clone();

        let handle = tokio::spawn(async move {
            eprintln!("[{}/{}] {} 等待信号量...", idx + 1, total, task.filename);
            let permit = match sem.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    eprintln!("[{}/{}] {} 信号量已关闭", idx + 1, total, task.filename);
                    return;
                }
            };
            eprintln!("[{}/{}] {} 获取到信号量, 开始处理", idx + 1, total, task.filename);
            if is_cancelled() {
                emit_progress(&app, &task.filename, &task.local_path, "pending", 0, 0, 0);
                return;
            }
            download_with_retry(&client, &task, &dest, retries, chunk_threads, &app, idx, total).await;
            let count = counter.fetch_add(1, Ordering::Relaxed) + 1;
            eprintln!("[{}/{}] {} 处理完成, 释放信号量 (已完成: {}/{})", idx + 1, total, task.filename, count, total);
            drop(permit);
        });

        handles.push(handle);
    }

    eprintln!("所有任务已派发, 等待完成...");
    for handle in handles {
        let _ = handle.await;
    }
    eprintln!("所有任务已完成");

    // Signal speed emitter to stop
    ALL_DONE.store(true, Ordering::Relaxed);

    // Wait for speed emitter to exit
    let _ = speed_handle.await;

    // Emit final 0 speed
    let _ = app.emit("download-speed", 0u64);

    Ok(())
}

async fn check_range_support(client: &reqwest::Client, url: &str) -> Option<u64> {
    let resp = client.head(url).send().await.ok()?;
    let headers = resp.headers();
    let accept_ranges = headers
        .get(reqwest::header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if accept_ranges != "bytes" {
        return None;
    }
    let content_length = headers
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())?;
    Some(content_length)
}

async fn download_file_chunked(
    client: &reqwest::Client,
    url: &str,
    local_path: &Path,
    task: &FileItem,
    app: &AppHandle,
    chunk_threads: usize,
    total_size: u64,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let chunk_size_per_thread = total_size / chunk_threads as u64;
    let downloaded_bytes = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();

    for i in 0..chunk_threads {
        let start = i as u64 * chunk_size_per_thread;
        let end = if i == chunk_threads - 1 {
            total_size - 1
        } else {
            (i as u64 + 1) * chunk_size_per_thread - 1
        };

        let part_path = local_path.with_extension(format!("part{}", i));
        let client = client.clone();
        let url = url.to_string();
        let downloaded_bytes = downloaded_bytes.clone();
        let app = app.clone();
        let filename = task.filename.clone();
        let task_local_path = task.local_path.clone();

        let handle = tokio::spawn(async move {
            let resp = client
                .get(&url)
                .header("Range", format!("bytes={}-{}", start, end))
                .send()
                .await?;

            if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                let err: Box<dyn std::error::Error + Send + Sync> =
                    format!("服务器返回 {} 而非 206", resp.status()).into();
                return Err(err);
            }

            let mut file = fs::File::create(&part_path).await?;
            let mut stream = resp.bytes_stream();

            while let Some(chunk_result) = stream.next().await {
                if is_cancelled() {
                    drop(file);
                    let _ = fs::remove_file(&part_path).await;
                    return Err("cancelled".into());
                }

                let chunk = chunk_result?;
                file.write_all(&chunk).await?;
                let chunk_len = chunk.len() as u64;
                TOTAL_BYTES.fetch_add(chunk_len, Ordering::Relaxed);
                let prev = downloaded_bytes.fetch_add(chunk_len, Ordering::Relaxed);
                let current = prev + chunk_len;

                // Emit progress every CHUNK_SIZE
                if total_size > 0 && (current / CHUNK_SIZE) > (prev / CHUNK_SIZE) {
                    let percent = ((current as f64 / total_size as f64) * 100.0) as u32;
                    emit_progress(
                        &app,
                        &filename,
                        &task_local_path,
                        "downloading",
                        percent,
                        current,
                        total_size,
                    );
                }
            }

            file.flush().await?;
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        });

        handles.push(handle);
    }

    // Wait for all chunks to complete
    let mut first_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;

    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("分片任务panic: {}", e).into());
                }
            }
        }
    }

    if let Some(e) = first_err {
        return Err(e);
    }

    // Merge part files into the final file
    let tmp_path = get_tmp_path(local_path);
    let mut out_file = fs::File::create(&tmp_path).await?;
    let mut buf = vec![0u8; 8192];

    for i in 0..chunk_threads {
        let part_path = local_path.with_extension(format!("part{}", i));
        let mut part_file = fs::File::open(&part_path).await?;
        loop {
            let n = part_file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            out_file.write_all(&buf[..n]).await?;
        }
    }

    out_file.flush().await?;
    drop(out_file);

    // Remove part files
    for i in 0..chunk_threads {
        let part_path = local_path.with_extension(format!("part{}", i));
        let _ = fs::remove_file(&part_path).await;
    }

    // Rename tmp to final
    if local_path.exists() {
        fs::remove_file(local_path).await?;
    }
    fs::rename(&tmp_path, local_path).await?;

    Ok(total_size)
}

async fn clean_part_files(local_path: &Path) {
    for i in 0..256 {
        let part = local_path.with_extension(format!("part{}", i));
        if part.exists() {
            let _ = fs::remove_file(&part).await;
        } else {
            break;
        }
    }
}

async fn download_with_retry(
    client: &reqwest::Client,
    task: &FileItem,
    dest_dir: &str,
    retries: usize,
    chunk_threads: usize,
    app: &AppHandle,
    idx: usize,
    total: usize,
) {
    let local_path = Path::new(dest_dir).join(&task.local_path);

    if let Some(parent) = local_path.parent() {
        let _ = fs::create_dir_all(parent).await;
    }

    // Check if file already exists with matching MD5
    if local_path.exists() {
        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            compute_file_md5_streaming(&local_path),
        )
        .await
        {
            Ok(Ok(existing_md5)) if existing_md5 == task.md5 => {
                let size = fs::metadata(&local_path).await.map(|m| m.len()).unwrap_or(0);
                emit_progress(app, &task.filename, &task.local_path, "skipped", 100, size, size);
                eprintln!("[{}/{}] {} 跳过 (MD5匹配)", idx + 1, total, task.filename);
                return;
            }
            _ => {
                let _ = fs::remove_file(&local_path).await;
            }
        }
    }

    let mut attempt = 0;
    while attempt <= retries {
        if is_cancelled() {
            emit_progress(app, &task.filename, &task.local_path, "pending", 0, 0, 0);
            return;
        }

        attempt += 1;
        emit_progress(app, &task.filename, &task.local_path, "downloading", 0, 0, 0);
        eprintln!("[{}/{}] {} 开始下载 (尝试{}/{})", idx + 1, total, task.filename, attempt, retries + 1);

        // Try chunked download if chunk_threads > 1
        if chunk_threads > 1 && !task.url.is_empty() {
            if let Some(file_size) = check_range_support(client, &task.url).await {
                if file_size >= 1_048_576 {
                    // >= 1MB, use chunked download
                    match download_file_chunked(client, &task.url, &local_path, task, app, chunk_threads, file_size).await {
                        Ok(total_size) => {
                            // Chunked download succeeded, verify MD5 in the caller's loop
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(60),
                                compute_file_md5_streaming(&local_path),
                            )
                            .await
                            {
                                Ok(Ok(downloaded_md5)) if downloaded_md5 == task.md5 => {
                                    emit_progress(app, &task.filename, &task.local_path, "completed", 100, total_size, total_size);
                                    eprintln!("[{}/{}] {} 完成 (分片下载)", idx + 1, total, task.filename);
                                    return;
                                }
                                _ => {
                                    let _ = fs::remove_file(&local_path).await;
                                    if attempt > retries {
                                        emit_progress(app, &task.filename, &task.local_path, "failed", 0, 0, 0);
                                        eprintln!("[{}/{}] {} 失败 (MD5不匹配)", idx + 1, total, task.filename);
                                        return;
                                    }
                                    continue;
                                }
                            }
                        }
                        Err(e) => {
                            // Chunked download failed, clean up and fall back to single-thread
                            eprintln!("分片下载失败，回退到单线程: {}", e);
                            clean_part_files(&local_path).await;
                            // Fall through to download_file_streaming below
                        }
                    }
                } else {
                    eprintln!("[{}/{}] {} 文件过小({} bytes)，跳过分片", idx + 1, total, task.filename, file_size);
                }
            } else {
                // Range not supported, emit warning
                let _ = app.emit("download-warning", format!("{} 不支持分片下载，已回退为单线程", task.filename));
                eprintln!("[{}/{}] {} 不支持 Range 请求，使用单线程下载", idx + 1, total, task.filename);
            }
        }

        // Fallback or default: single-thread download
        match download_file_streaming(client, &task.url, &local_path, task, app).await {
            Ok(total_size) => {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(60),
                    compute_file_md5_streaming(&local_path),
                )
                .await
                {
                    Ok(Ok(downloaded_md5)) if downloaded_md5 == task.md5 => {
                        emit_progress(app, &task.filename, &task.local_path, "completed", 100, total_size, total_size);
                        eprintln!("[{}/{}] {} 完成", idx + 1, total, task.filename);
                        return;
                    }
                    _ => {
                        let _ = fs::remove_file(&local_path).await;
                        if attempt > retries {
                            emit_progress(app, &task.filename, &task.local_path, "failed", 0, 0, 0);
                            eprintln!("[{}/{}] {} 失败 (MD5不匹配)", idx + 1, total, task.filename);
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                let tmp_path = get_tmp_path(&local_path);
                let _ = fs::remove_file(&tmp_path).await;
                eprintln!("[{}/{}] {} 下载出错: {}", idx + 1, total, task.filename, e);
                if attempt > retries {
                    emit_progress(app, &task.filename, &task.local_path, "failed", 0, 0, 0);
                    return;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }
    }
}

fn get_tmp_path(local_path: &Path) -> PathBuf {
    local_path.with_extension(format!(
        "tmp.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ))
}

async fn download_file_streaming(
    client: &reqwest::Client,
    url: &str,
    local_path: &Path,
    task: &FileItem,
    app: &AppHandle,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()).into());
    }

    let total_size = response.content_length().unwrap_or(0);
    let tmp_path = get_tmp_path(local_path);

    let mut file = fs::File::create(&tmp_path).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emitted_bytes: u64 = 0;

    while let Some(chunk_result) = stream.next().await {
        if is_cancelled() {
            drop(file);
            let _ = fs::remove_file(&tmp_path).await;
            return Err("cancelled".into());
        }

        let chunk = chunk_result?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        TOTAL_BYTES.fetch_add(chunk.len() as u64, Ordering::Relaxed);

        if total_size > 0
            && (downloaded - last_emitted_bytes >= CHUNK_SIZE || downloaded == total_size)
        {
            let percent = ((downloaded as f64 / total_size as f64) * 100.0) as u32;
            emit_progress(app, &task.filename, &task.local_path, "downloading", percent, downloaded, total_size);
            last_emitted_bytes = downloaded;
        }
    }

    file.flush().await?;
    drop(file);

    if local_path.exists() {
        fs::remove_file(local_path).await?;
    }
    fs::rename(&tmp_path, local_path).await?;

    Ok(total_size)
}

async fn compute_file_md5_streaming(path: &Path) -> Result<String, std::io::Error> {
    let mut file = fs::File::open(path).await?;
    let mut hasher = Md5::new();
    let mut buf = vec![0u8; 8192];

    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn emit_progress(app: &AppHandle, filename: &str, local_path: &str, status: &str, progress: u32, downloaded_bytes: u64, total_bytes: u64) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            filename: filename.to_string(),
            local_path: local_path.to_string(),
            status: status.to_string(),
            progress,
            downloaded_bytes,
            total_bytes,
        },
    );
}
