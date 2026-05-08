export interface FileItem {
  filename: string;
  md5: string;
  group: string;
  url: string;
  local_path: string;
  status: "pending" | "downloading" | "completed" | "skipped" | "failed" | "unzipping" | "unzipped";
  progress: number;
  downloaded_bytes: number;
  total_bytes: number;
}

export interface EnvInfo {
  number: string;
  name: string;
  zk_address: string;
}

export interface DownloadProgress {
  filename: string;
  local_path: string;
  status: string;
  progress: number;
  downloaded_bytes: number;
  total_bytes: number;
}

export interface FetchResult {
  json: Record<string, unknown>;
  base_url: string;
}

export type RunMode = "download_only" | "unzip_only" | "both";
