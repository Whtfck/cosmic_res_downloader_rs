use crate::models::FileItem;
use serde_json::Value;

const APPSTORE_SUBDIRS: &[&str] = &["bos", "biz", "trd", "cus"];
const DEFAULT_FILTER: &str = r"\d+(?:bk|bak)|(?:bk|bak)\d+";

pub fn parse_file_list(json: &Value, base_url: &str, filter_pattern: &str) -> Vec<FileItem> {
    let pattern = if filter_pattern.is_empty() {
        DEFAULT_FILTER
    } else {
        filter_pattern
    };

    let re = regex::Regex::new(pattern).ok();

    let mut items = Vec::new();

    // Process webapp
    if let Some(webapp) = json.get("webapp") {
        if let (Some(path), Some(files)) = (webapp.get("path"), webapp.get("files")) {
            let path_str = path.as_str().unwrap_or("");
            if let Some(files_map) = files.as_object() {
                for (filename, md5_val) in files_map {
                    if should_filter(filename, re.as_ref()) {
                        continue;
                    }
                    let md5 = md5_val.as_str().unwrap_or("").to_string();
                    let url = format!("{}{}/{}", base_url, path_str, filename);
                    items.push(FileItem {
                        filename: filename.clone(),
                        md5,
                        group: "webapp".to_string(),
                        url,
                        local_path: format!("webapp/{}", filename),
                        status: "pending".to_string(),
                        progress: 0,
                        downloaded_bytes: 0,
                        total_bytes: 0,
                    });
                }
            }
        }
    }

    // Process appstore
    if let Some(appstore) = json.get("appstore") {
        let base_path = appstore
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or("");

        for &subdir in APPSTORE_SUBDIRS {
            if let Some(section) = appstore.get(subdir) {
                if let Some(files_map) = section.as_object() {
                    for (filename, md5_val) in files_map {
                        if should_filter(filename, re.as_ref()) {
                            continue;
                        }
                        let md5 = md5_val.as_str().unwrap_or("").to_string();
                        let url = format!("{}/{}/{}/{}", base_url, base_path, subdir, filename);
                        items.push(FileItem {
                            filename: filename.clone(),
                            md5,
                            group: format!("appstore/{}", subdir),
                            url,
                            local_path: format!("appstore/{}/{}", subdir, filename),
                            status: "pending".to_string(),
                            progress: 0,
                            downloaded_bytes: 0,
                            total_bytes: 0,
                        });
                    }
                }
            }
        }
    }

    items
}

fn should_filter(filename: &str, re: Option<&regex::Regex>) -> bool {
    // Multiple dots (e.g. "file.20240428bk.zip")
    if filename.matches('.').count() > 1 {
        return true;
    }
    // Regex pattern match
    if let Some(re) = re {
        if re.is_match(filename) {
            return true;
        }
    }
    false
}
