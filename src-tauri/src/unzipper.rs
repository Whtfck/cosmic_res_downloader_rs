use crate::downloader;
use crate::models::DownloadProgress;
use crate::parser;
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::fs;
use tokio::sync::Semaphore;

/// Unified unzip: webapp uses regex only, appstore uses whitelist + regex
pub async fn unzip_with_filters(
    dest_dir: &str,
    whitelist: Option<&HashSet<String>>,
    filter_pattern: &str,
    concurrency: usize,
    app: &AppHandle,
) -> Result<(), String> {
    let base = Path::new(dest_dir);
    let re = Regex::new(if filter_pattern.is_empty() {
        r"\d+(?:bk|bak)|(?:bk|bak)\d+"
    } else {
        filter_pattern
    })
    .ok();

    // Collect all zip files to unzip across all directories
    let mut all_jobs: Vec<(std::path::PathBuf, std::path::PathBuf, String, String)> = Vec::new();

    // webapp -> static-file-service/ (regex filter only)
    let webapp_dir = base.join("webapp");
    if webapp_dir.exists() {
        let target = base.join("static-file-service");
        let _ = fs::create_dir_all(&target).await;
        collect_zip_jobs(&webapp_dir, &target, base, None, re.as_ref(), &mut all_jobs).await;
    }

    // appstore/{sub} -> mservice-cosmic/lib/{sub}/ (whitelist + regex)
    let appstore_dir = base.join("appstore");
    if appstore_dir.exists() {
        for subdir in &["bos", "biz", "trd", "cus"] {
            let sub_path = appstore_dir.join(subdir);
            if sub_path.exists() {
                let target = base.join("mservice-cosmic").join("lib").join(subdir);
                let _ = fs::create_dir_all(&target).await;
                collect_zip_jobs(&sub_path, &target, base, whitelist, re.as_ref(), &mut all_jobs).await;
            }
        }
    }

    let total = all_jobs.len() as u32;
    eprintln!("解压总计: {} 个文件, 并发: {}", total, concurrency);

    // Emit the actual total so the frontend uses the correct denominator
    let _ = app.emit("unzip-total", total);
    let done = Arc::new(AtomicU32::new(0));
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::new();

    for (zip_path, dest_dir_buf, name, rel_path) in all_jobs {
        if downloader::is_cancelled() {
            emit_progress(app, &name, &rel_path, "pending", 0);
            continue;
        }

        emit_progress(app, &name, &rel_path, "unzipping", 0);

        let sem = semaphore.clone();
        let app = app.clone();
        let done = done.clone();
        let name_owned = name.clone();
        let rel_path_owned = rel_path.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let result = tokio::task::spawn_blocking(move || {
                unzip_file(&zip_path, &dest_dir_buf)
            })
            .await
            .map_err(|e| format!("解压任务失败: {}", e))?;

            let count = done.fetch_add(1, Ordering::Relaxed) + 1;
            let percent = if total > 0 { (count * 100 / total) as u32 } else { 100 };
            emit_progress(&app, &name_owned, &rel_path_owned, "unzipped", percent);
            result
        });

        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn collect_zip_jobs(
    src_dir: &Path,
    dest_dir: &Path,
    base_dir: &Path,
    whitelist: Option<&HashSet<String>>,
    re: Option<&Regex>,
    jobs: &mut Vec<(std::path::PathBuf, std::path::PathBuf, String, String)>,
) {
    let mut entries = match fs::read_dir(src_dir).await {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.extension().map_or(false, |ext| ext == "zip") {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if parser::should_filter(&name, re) {
            eprintln!("解压正则跳过: {}", name);
            continue;
        }

        if let Some(wl) = whitelist {
            if !wl.contains(&name) {
                eprintln!("解压白名单跳过: {}", name);
                continue;
            }
        }

        let rel_path = path
            .strip_prefix(base_dir)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();

        jobs.push((path, dest_dir.to_path_buf(), name, rel_path));
    }
}

fn unzip_file(zip_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file =
        std::fs::File::open(zip_path).map_err(|e| format!("打开zip失败: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("读取zip失败: {}", e))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取zip条目失败: {}", e))?;

        let out_path = dest_dir.join(entry.mangled_name());

        if entry.name().ends_with('/') {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("创建目录失败: {}", e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录失败: {}", e))?;
            }
            let mut out_file = std::fs::File::create(&out_path)
                .map_err(|e| format!("创建文件失败: {}", e))?;
            std::io::copy(&mut entry, &mut out_file)
                .map_err(|e| format!("写入文件失败: {}", e))?;
        }
    }

    Ok(())
}

fn emit_progress(
    app: &AppHandle,
    filename: &str,
    local_path: &str,
    status: &str,
    progress: u32,
) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            filename: filename.to_string(),
            local_path: local_path.to_string(),
            status: status.to_string(),
            progress,
            downloaded_bytes: 0,
            total_bytes: 0,
        },
    );
}
