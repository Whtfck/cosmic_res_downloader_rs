use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use tokio::fs;

const DEFAULT_FILTER: &str = r"\d+(?:bk|bak)|(?:bk|bak)\d+";

fn should_filter(filename: &str, re: Option<&Regex>) -> bool {
    if filename.matches('.').count() > 1 {
        return true;
    }
    if let Some(re) = re {
        if re.is_match(filename) {
            return true;
        }
    }
    false
}

/// Parse a single XML file and extract zip filenames from <zips><zip>xxx</zip></zips>
fn parse_xml_whitelist(xml_path: &Path) -> Result<Vec<String>, String> {
    let content = std::fs::read_to_string(xml_path)
        .map_err(|e| format!("读取XML失败 {}: {}", xml_path.display(), e))?;

    let mut reader = Reader::from_str(&content);
    let mut in_zips = false;
    let mut in_zip = false;
    let mut whitelist = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if e.name().as_ref() == b"zips" {
                    in_zips = true;
                } else if e.name().as_ref() == b"zip" && in_zips {
                    in_zip = true;
                } else if in_zips {
                    in_zips = false;
                }
            }
            Ok(Event::Text(e)) => {
                if in_zip {
                    let text = e.unescape().map_err(|e| format!("XML文本解析失败: {}", e))?;
                    let name = text.trim().to_string();
                    if !name.is_empty() {
                        whitelist.push(name);
                    }
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"zip" {
                    in_zip = false;
                } else if e.name().as_ref() == b"zips" {
                    in_zips = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML解析错误 {}: {}", xml_path.display(), e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(whitelist)
}

/// Scan dest_dir for .xml files, parse whitelist, apply regex filter
pub async fn collect_xml_whitelist(dest_dir: &str, filter_pattern: &str) -> HashSet<String> {
    let base = Path::new(dest_dir);
    let mut whitelist = HashSet::new();

    let pattern = if filter_pattern.is_empty() {
        DEFAULT_FILTER
    } else {
        filter_pattern
    };
    let re = Regex::new(pattern).ok();

    let dirs = ["webapp", "appstore/bos", "appstore/biz", "appstore/trd", "appstore/cus"];
    for dir in &dirs {
        let scan_dir = base.join(dir);
        if !scan_dir.exists() {
            continue;
        }
        let mut entries = match fs::read_dir(&scan_dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "xml") {
                match parse_xml_whitelist(&path) {
                    Ok(names) => {
                        for name in names {
                            // 先用原始XML名做正则检查，再加.zip后缀
                            if should_filter(&name, re.as_ref()) {
                                continue;
                            }
                            let filename = if name.ends_with(".zip") {
                                name
                            } else {
                                format!("{}.zip", name)
                            };
                            whitelist.insert(filename);
                        }
                    }
                    Err(e) => {
                        eprintln!("跳过无效XML: {}", e);
                    }
                }
            }
        }
    }

    eprintln!("XML白名单(正则过滤后): {:?}", whitelist);
    whitelist
}
