//! Log file tail + live follow helpers for the admin UI WebSocket.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

pub const DEFAULT_LOG_LINES: usize = 200;
pub const MAX_LOG_LINES: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    Core,
    Node,
}

impl LogSource {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "core" => Some(Self::Core),
            "node" => Some(Self::Node),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Node => "node",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LogTail {
    pub source: String,
    pub path: String,
    pub lines: Vec<String>,
    pub truncated: bool,
    pub exists: bool,
    pub mtime_unix: Option<u64>,
    /// Byte offset after the tailed content (for live follow).
    #[serde(skip)]
    pub follow_offset: u64,
}

pub fn clamp_lines(requested: Option<usize>) -> usize {
    requested
        .unwrap_or(DEFAULT_LOG_LINES)
        .clamp(1, MAX_LOG_LINES)
}

pub fn path_for(source: LogSource, layout: &crate::data_dir::DataLayout) -> PathBuf {
    match source {
        LogSource::Core => layout.core_log_path(),
        LogSource::Node => layout.node_log_path(),
    }
}

pub fn read_tail(path: &Path, max_lines: usize) -> std::io::Result<LogTail> {
    let source = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("log")
        .to_string();

    if !path.exists() {
        return Ok(LogTail {
            source,
            path: path.display().to_string(),
            lines: Vec::new(),
            truncated: false,
            exists: false,
            mtime_unix: None,
            follow_offset: 0,
        });
    }

    let meta = std::fs::metadata(path)?;
    let len = meta.len();
    let mtime_unix = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let (lines, follow_offset) = tail_lines_with_offset(path, max_lines, len)?;
    let truncated = lines.len() >= max_lines;

    Ok(LogTail {
        source,
        path: path.display().to_string(),
        lines,
        truncated,
        exists: true,
        mtime_unix,
        follow_offset,
    })
}

/// Read newly appended bytes since `offset`. Returns complete lines and the new offset.
/// An incomplete trailing line is kept in `pending` across calls.
pub fn read_new_lines(
    path: &Path,
    offset: u64,
    pending: &mut String,
) -> std::io::Result<(Vec<String>, u64)> {
    if !path.exists() {
        return Ok((Vec::new(), offset));
    }
    let mut file = File::open(path)?;
    let len = file.seek(SeekFrom::End(0))?;
    if len < offset {
        // Log rotated/truncated — restart from beginning.
        *pending = String::new();
        return Ok((Vec::new(), 0));
    }
    if len == offset {
        return Ok((Vec::new(), offset));
    }

    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; (len - offset) as usize];
    file.read_exact(&mut buf)?;
    pending.push_str(&String::from_utf8_lossy(&buf));

    let mut lines = Vec::new();
    while let Some(idx) = pending.find('\n') {
        let mut line = pending.drain(..=idx).collect::<String>();
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        lines.push(line);
    }
    Ok((lines, len))
}

fn tail_lines_with_offset(
    path: &Path,
    max_lines: usize,
    len: u64,
) -> std::io::Result<(Vec<String>, u64)> {
    if len == 0 {
        return Ok((Vec::new(), 0));
    }

    let mut file = File::open(path)?;
    const CHUNK: u64 = 8 * 1024;
    let mut pos = len;
    let mut buffer = Vec::new();
    let mut newline_count = 0usize;

    while pos > 0 && newline_count <= max_lines {
        let read_size = CHUNK.min(pos);
        pos -= read_size;
        file.seek(SeekFrom::Start(pos))?;
        let mut chunk = vec![0u8; read_size as usize];
        file.read_exact(&mut chunk)?;
        newline_count += chunk.iter().filter(|&&b| b == b'\n').count();
        buffer.splice(0..0, chunk);
        if pos == 0 {
            break;
        }
    }

    let text = String::from_utf8_lossy(&buffer);
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    if lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }
    Ok((lines, len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn empty_missing_file() {
        let dir = std::env::temp_dir().join(format!("wqc-miner-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core.log");
        let tail = read_tail(&path, 50).unwrap();
        assert!(!tail.exists);
        assert!(tail.lines.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_last_n_lines() {
        let dir = std::env::temp_dir().join(format!("wqc-miner-log-n-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("node.log");
        {
            let mut f = File::create(&path).unwrap();
            for i in 0..100 {
                writeln!(f, "line-{i}").unwrap();
            }
        }
        let tail = read_tail(&path, 5).unwrap();
        assert!(tail.exists);
        assert_eq!(tail.lines.len(), 5);
        assert_eq!(tail.lines[0], "line-95");
        assert_eq!(tail.lines[4], "line-99");
        assert!(tail.truncated);
        assert_eq!(tail.follow_offset, std::fs::metadata(&path).unwrap().len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn follow_appends() {
        let dir = std::env::temp_dir().join(format!("wqc-miner-log-f-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core.log");
        {
            let mut f = File::create(&path).unwrap();
            writeln!(f, "a").unwrap();
        }
        let tail = read_tail(&path, 50).unwrap();
        let mut pending = String::new();
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(f, "b").unwrap();
            write!(f, "partial").unwrap();
        }
        let (lines, offset) = read_new_lines(&path, tail.follow_offset, &mut pending).unwrap();
        assert_eq!(lines, vec!["b".to_string()]);
        assert_eq!(pending, "partial");
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(f, "-done").unwrap();
        }
        let (lines2, _) = read_new_lines(&path, offset, &mut pending).unwrap();
        assert_eq!(lines2, vec!["partial-done".to_string()]);
        assert!(pending.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clamp_lines_bounds() {
        assert_eq!(clamp_lines(None), DEFAULT_LOG_LINES);
        assert_eq!(clamp_lines(Some(0)), 1);
        assert_eq!(clamp_lines(Some(5000)), MAX_LOG_LINES);
    }
}
