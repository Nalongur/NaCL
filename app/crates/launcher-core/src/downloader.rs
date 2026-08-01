use reqwest::blocking::{Client, Response};
use reqwest::header::{
    HeaderValue, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, ETAG, IF_RANGE, LAST_MODIFIED,
    RANGE,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

const MIN_PIECE_SIZE: u64 = 2 * 1024 * 1024;
const MAX_PIECE_SIZE: u64 = 16 * 1024 * 1024;
const IO_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadControl {
    Running,
    Paused,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadEngine {
    Cache,
    Segmented,
    Streaming,
}

#[derive(Debug, Clone, Copy)]
pub struct TransferUpdate {
    pub downloaded_bytes: u64,
    pub speed_bytes_per_second: u64,
    pub engine: DownloadEngine,
    pub active_connections: u8,
}

#[derive(Debug, Clone)]
pub struct DownloadRequest<'a> {
    pub label: &'a str,
    pub url: &'a str,
    pub expected_size: u64,
    pub identity: &'a str,
    pub temporary_path: &'a Path,
}

#[derive(Debug, Clone, Copy)]
pub struct DownloadOptions {
    pub segment_connections: u8,
    pub segment_threshold_bytes: u64,
    pub retry_count: u8,
    pub speed_limit_kib_per_second: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct DownloadOutcome {
    pub downloaded_bytes: u64,
    pub engine: DownloadEngine,
}

#[derive(Debug)]
pub enum DownloadError {
    Cancelled,
    Network(String),
    HttpStatus {
        url: String,
        status: u16,
    },
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    InvalidContentRange(String),
    SizeMismatch {
        expected: u64,
        actual: u64,
    },
    State(String),
    UnsupportedRange(String),
}

impl DownloadError {
    fn permits_streaming_fallback(&self) -> bool {
        matches!(
            self,
            Self::Network(_)
                | Self::HttpStatus { .. }
                | Self::InvalidContentRange(_)
                | Self::UnsupportedRange(_)
        )
    }
}

impl fmt::Display for DownloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(formatter, "下载已取消"),
            Self::Network(error) => write!(formatter, "网络请求失败：{error}"),
            Self::HttpStatus { url, status } => {
                write!(formatter, "下载 {url} 时返回 HTTP {status}")
            }
            Self::Io {
                action,
                path,
                source,
            } => {
                write!(formatter, "无法{action} {}：{source}", path.display())
            }
            Self::InvalidContentRange(value) => {
                write!(formatter, "服务器返回了无效的 Content-Range：{value}")
            }
            Self::SizeMismatch { expected, actual } => {
                write!(
                    formatter,
                    "下载大小不匹配：预期 {expected} 字节，实际 {actual} 字节"
                )
            }
            Self::State(message) => write!(formatter, "断点状态无效：{message}"),
            Self::UnsupportedRange(message) => write!(formatter, "服务器不支持可靠分片：{message}"),
        }
    }
}

impl std::error::Error for DownloadError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResumeState {
    schema_version: u8,
    url: String,
    identity: String,
    total_size: u64,
    piece_size: u64,
    completed: Vec<bool>,
    etag: Option<String>,
    last_modified: Option<String>,
}

#[derive(Debug, Clone)]
struct ProbeResult {
    total_size: u64,
    etag: Option<String>,
    last_modified: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct Piece {
    index: usize,
    start: u64,
    end: u64,
}

impl Piece {
    fn len(self) -> u64 {
        self.end - self.start + 1
    }
}

pub fn download<F, C>(
    client: &Client,
    request: DownloadRequest<'_>,
    options: DownloadOptions,
    control: &C,
    progress: &F,
) -> Result<DownloadOutcome, DownloadError>
where
    F: Fn(TransferUpdate) + Sync,
    C: Fn() -> DownloadControl + Sync,
{
    wait_for_control(control)?;
    let use_segments =
        request.expected_size >= options.segment_threshold_bytes && options.segment_connections > 1;
    if use_segments {
        match segmented_download(client, &request, options, control, progress) {
            Ok(bytes) => {
                return Ok(DownloadOutcome {
                    downloaded_bytes: bytes,
                    engine: DownloadEngine::Segmented,
                });
            }
            Err(error) if error.permits_streaming_fallback() => {
                cleanup_partial_state(request.temporary_path);
                progress(TransferUpdate {
                    downloaded_bytes: 0,
                    speed_bytes_per_second: 0,
                    engine: DownloadEngine::Streaming,
                    active_connections: 1,
                });
            }
            Err(error) => return Err(error),
        }
    }

    let bytes = streaming_download(client, &request, options, control, progress)?;
    Ok(DownloadOutcome {
        downloaded_bytes: bytes,
        engine: DownloadEngine::Streaming,
    })
}

pub fn cleanup_partial_state(temporary: &Path) {
    let _ = fs::remove_file(temporary);
    let _ = fs::remove_file(resume_path(temporary));
}

fn segmented_download<F, C>(
    client: &Client,
    request: &DownloadRequest<'_>,
    options: DownloadOptions,
    control: &C,
    progress: &F,
) -> Result<u64, DownloadError>
where
    F: Fn(TransferUpdate) + Sync,
    C: Fn() -> DownloadControl + Sync,
{
    let probe = probe_range_support(client, request.url)?;
    if request.expected_size > 0 && probe.total_size != request.expected_size {
        return Err(DownloadError::SizeMismatch {
            expected: request.expected_size,
            actual: probe.total_size,
        });
    }
    let connection_count = options.segment_connections.clamp(2, 16);
    let piece_size = choose_piece_size(probe.total_size, connection_count);
    let pieces = build_pieces(probe.total_size, piece_size);
    let state_path = resume_path(request.temporary_path);
    let mut state = load_compatible_state(&state_path, request, &probe, piece_size, pieces.len())?
        .unwrap_or_else(|| ResumeState {
            schema_version: 1,
            url: request.url.to_string(),
            identity: request.identity.to_string(),
            total_size: probe.total_size,
            piece_size,
            completed: vec![false; pieces.len()],
            etag: probe.etag.clone(),
            last_modified: probe.last_modified.clone(),
        });

    if !request.temporary_path.is_file()
        || fs::metadata(request.temporary_path)
            .map_err(|source| io_error("读取临时文件", request.temporary_path, source))?
            .len()
            != probe.total_size
    {
        state.completed.fill(false);
        let file = File::create(request.temporary_path)
            .map_err(|source| io_error("创建分片临时文件", request.temporary_path, source))?;
        file.set_len(probe.total_size)
            .map_err(|source| io_error("预分配分片临时文件", request.temporary_path, source))?;
        file.sync_all()
            .map_err(|source| io_error("同步分片临时文件", request.temporary_path, source))?;
        save_state(&state_path, &state)?;
    }

    let completed_bytes = pieces
        .iter()
        .filter(|piece| state.completed[piece.index])
        .map(|piece| piece.len())
        .sum::<u64>();
    let queue = pieces
        .iter()
        .filter(|piece| !state.completed[piece.index])
        .copied()
        .collect::<VecDeque<_>>();
    if queue.is_empty() {
        let _ = fs::remove_file(&state_path);
        progress(TransferUpdate {
            downloaded_bytes: probe.total_size,
            speed_bytes_per_second: 0,
            engine: DownloadEngine::Segmented,
            active_connections: 0,
        });
        return Ok(probe.total_size);
    }

    let file = Arc::new(
        OpenOptions::new()
            .write(true)
            .open(request.temporary_path)
            .map_err(|source| io_error("打开分片临时文件", request.temporary_path, source))?,
    );
    let queue = Arc::new(Mutex::new(queue));
    let state = Arc::new(Mutex::new(state));
    let first_error = Arc::new(Mutex::new(None::<DownloadError>));
    let stop = Arc::new(AtomicBool::new(false));
    let downloaded = Arc::new(AtomicU64::new(completed_bytes));
    let active = Arc::new(AtomicU64::new(0));
    let transfer_rate = Arc::new(Mutex::new(TransferRate::new()));
    let limiter = Arc::new(RateLimiter::new(options.speed_limit_kib_per_second));
    let validator = probe
        .etag
        .as_deref()
        .filter(|etag| is_strong_entity_tag(etag))
        .or(probe.last_modified.as_deref())
        .map(str::to_owned);

    progress(TransferUpdate {
        downloaded_bytes: completed_bytes,
        speed_bytes_per_second: 0,
        engine: DownloadEngine::Segmented,
        active_connections: connection_count,
    });

    let state_path_ref = state_path.as_path();
    thread::scope(|scope| {
        for _ in 0..connection_count {
            let file = Arc::clone(&file);
            let queue = Arc::clone(&queue);
            let state = Arc::clone(&state);
            let first_error = Arc::clone(&first_error);
            let stop = Arc::clone(&stop);
            let downloaded = Arc::clone(&downloaded);
            let active = Arc::clone(&active);
            let transfer_rate = Arc::clone(&transfer_rate);
            let limiter = Arc::clone(&limiter);
            let validator = validator.clone();
            scope.spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let piece = {
                        let mut queue = queue
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        queue.pop_front()
                    };
                    let Some(piece) = piece else { return };
                    active.fetch_add(1, Ordering::Relaxed);
                    let result = download_piece(
                        client,
                        request,
                        piece,
                        validator.as_deref(),
                        options.retry_count,
                        control,
                        &file,
                        &downloaded,
                        &active,
                        &transfer_rate,
                        &limiter,
                        progress,
                    );
                    active.fetch_sub(1, Ordering::Relaxed);
                    match result {
                        Ok(()) => {
                            let save_result = {
                                let mut state = state
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                                state.completed[piece.index] = true;
                                save_state(state_path_ref, &state)
                            };
                            if let Err(error) = save_result {
                                record_first_error(&first_error, &stop, error);
                                return;
                            }
                        }
                        Err(error) => {
                            record_first_error(&first_error, &stop, error);
                            return;
                        }
                    }
                }
            });
        }
    });

    if let Some(error) = first_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    {
        return Err(error);
    }
    file.sync_all()
        .map_err(|source| io_error("同步分片临时文件", request.temporary_path, source))?;
    let actual = downloaded.load(Ordering::Relaxed);
    if actual != probe.total_size {
        return Err(DownloadError::SizeMismatch {
            expected: probe.total_size,
            actual,
        });
    }
    let _ = fs::remove_file(state_path);
    progress(TransferUpdate {
        downloaded_bytes: actual,
        speed_bytes_per_second: 0,
        engine: DownloadEngine::Segmented,
        active_connections: 0,
    });
    Ok(actual)
}

#[allow(clippy::too_many_arguments)]
fn download_piece<F, C>(
    client: &Client,
    request: &DownloadRequest<'_>,
    piece: Piece,
    validator: Option<&str>,
    retry_count: u8,
    control: &C,
    file: &File,
    downloaded: &AtomicU64,
    active: &AtomicU64,
    transfer_rate: &Mutex<TransferRate>,
    limiter: &RateLimiter,
    progress: &F,
) -> Result<(), DownloadError>
where
    F: Fn(TransferUpdate) + Sync,
    C: Fn() -> DownloadControl + Sync,
{
    let mut last_error = None;
    for _ in 0..=retry_count {
        wait_for_control(control)?;
        let mut request_builder = client
            .get(request.url)
            .header(ACCEPT_ENCODING, "identity")
            .header(RANGE, format!("bytes={}-{}", piece.start, piece.end));
        if let Some(validator) = validator.and_then(|value| HeaderValue::from_str(value).ok()) {
            request_builder = request_builder.header(IF_RANGE, validator);
        }
        let response = match request_builder.send() {
            Ok(response) => response,
            Err(error) => {
                last_error = Some(DownloadError::Network(error.to_string()));
                continue;
            }
        };
        let mut response = match validate_piece_response(response, piece) {
            Ok(response) => response,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        let mut offset = piece.start;
        let mut attempt_bytes = 0_u64;
        let mut buffer = [0_u8; IO_BUFFER_SIZE];
        let result = loop {
            if let Err(error) = wait_for_control(control) {
                break Err(error);
            }
            let count = match response.read(&mut buffer) {
                Ok(count) => count,
                Err(error) => break Err(DownloadError::Network(error.to_string())),
            };
            if count == 0 {
                break if attempt_bytes == piece.len() {
                    Ok(())
                } else {
                    Err(DownloadError::SizeMismatch {
                        expected: piece.len(),
                        actual: attempt_bytes,
                    })
                };
            }
            if attempt_bytes + count as u64 > piece.len() {
                break Err(DownloadError::SizeMismatch {
                    expected: piece.len(),
                    actual: attempt_bytes + count as u64,
                });
            }
            write_all_at(file, &buffer[..count], offset)
                .map_err(|source| io_error("写入分片临时文件", request.temporary_path, source))?;
            offset += count as u64;
            attempt_bytes += count as u64;
            let total = downloaded.fetch_add(count as u64, Ordering::Relaxed) + count as u64;
            limiter.throttle(count as u64);
            let speed = transfer_rate
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .record(count as u64);
            progress(TransferUpdate {
                downloaded_bytes: total,
                speed_bytes_per_second: speed,
                engine: DownloadEngine::Segmented,
                active_connections: active.load(Ordering::Relaxed).min(u64::from(u8::MAX)) as u8,
            });
        };
        match result {
            Ok(()) => return Ok(()),
            Err(DownloadError::Cancelled) => return Err(DownloadError::Cancelled),
            Err(error) => {
                downloaded.fetch_sub(attempt_bytes, Ordering::Relaxed);
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| DownloadError::Network("分片重试次数已耗尽".to_string())))
}

fn streaming_download<F, C>(
    client: &Client,
    request: &DownloadRequest<'_>,
    options: DownloadOptions,
    control: &C,
    progress: &F,
) -> Result<u64, DownloadError>
where
    F: Fn(TransferUpdate) + Sync,
    C: Fn() -> DownloadControl + Sync,
{
    wait_for_control(control)?;
    let mut response = client
        .get(request.url)
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|error| DownloadError::Network(error.to_string()))?;
    if !response.status().is_success() {
        return Err(DownloadError::HttpStatus {
            url: request.url.to_string(),
            status: response.status().as_u16(),
        });
    }
    let mut file = File::create(request.temporary_path)
        .map_err(|source| io_error("创建流式临时文件", request.temporary_path, source))?;
    let mut total = 0_u64;
    let mut transfer_rate = TransferRate::new();
    let limiter = RateLimiter::new(options.speed_limit_kib_per_second);
    let mut buffer = [0_u8; IO_BUFFER_SIZE];
    loop {
        wait_for_control(control)?;
        let count = response
            .read(&mut buffer)
            .map_err(|error| DownloadError::Network(error.to_string()))?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])
            .map_err(|source| io_error("写入流式临时文件", request.temporary_path, source))?;
        total = total.saturating_add(count as u64);
        limiter.throttle(count as u64);
        progress(TransferUpdate {
            downloaded_bytes: total,
            speed_bytes_per_second: transfer_rate.record(count as u64),
            engine: DownloadEngine::Streaming,
            active_connections: 1,
        });
    }
    file.sync_all()
        .map_err(|source| io_error("同步流式临时文件", request.temporary_path, source))?;
    if request.expected_size > 0 && total != request.expected_size {
        return Err(DownloadError::SizeMismatch {
            expected: request.expected_size,
            actual: total,
        });
    }
    progress(TransferUpdate {
        downloaded_bytes: total,
        speed_bytes_per_second: 0,
        engine: DownloadEngine::Streaming,
        active_connections: 0,
    });
    Ok(total)
}

fn probe_range_support(client: &Client, url: &str) -> Result<ProbeResult, DownloadError> {
    let response = client
        .get(url)
        .header(ACCEPT_ENCODING, "identity")
        .header(RANGE, "bytes=0-0")
        .send()
        .map_err(|error| DownloadError::Network(error.to_string()))?;
    if response.status().as_u16() != 206 {
        return Err(DownloadError::UnsupportedRange(format!(
            "探测请求返回 HTTP {}",
            response.status()
        )));
    }
    let content_range = header_string(&response, CONTENT_RANGE)
        .ok_or_else(|| DownloadError::InvalidContentRange("缺少响应头".to_string()))?;
    let (start, end, total_size) = parse_content_range(&content_range)?;
    if start != 0 || end != 0 || total_size == 0 {
        return Err(DownloadError::InvalidContentRange(content_range));
    }
    Ok(ProbeResult {
        total_size,
        etag: header_string(&response, ETAG),
        last_modified: header_string(&response, LAST_MODIFIED),
    })
}

fn validate_piece_response(response: Response, piece: Piece) -> Result<Response, DownloadError> {
    if response.status().as_u16() != 206 {
        return Err(DownloadError::UnsupportedRange(format!(
            "分片请求返回 HTTP {}",
            response.status()
        )));
    }
    let content_range = header_string(&response, CONTENT_RANGE)
        .ok_or_else(|| DownloadError::InvalidContentRange("缺少响应头".to_string()))?;
    let (start, end, _) = parse_content_range(&content_range)?;
    if start != piece.start || end != piece.end {
        return Err(DownloadError::InvalidContentRange(content_range));
    }
    if let Some(length) = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        if length != piece.len() {
            return Err(DownloadError::SizeMismatch {
                expected: piece.len(),
                actual: length,
            });
        }
    }
    Ok(response)
}

fn parse_content_range(value: &str) -> Result<(u64, u64, u64), DownloadError> {
    let value = value.trim();
    let ranges = value
        .strip_prefix("bytes ")
        .ok_or_else(|| DownloadError::InvalidContentRange(value.to_string()))?;
    let (range, total) = ranges
        .split_once('/')
        .ok_or_else(|| DownloadError::InvalidContentRange(value.to_string()))?;
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| DownloadError::InvalidContentRange(value.to_string()))?;
    let parsed = (
        start.parse::<u64>(),
        end.parse::<u64>(),
        total.parse::<u64>(),
    );
    match parsed {
        (Ok(start), Ok(end), Ok(total)) if start <= end && end < total => Ok((start, end, total)),
        _ => Err(DownloadError::InvalidContentRange(value.to_string())),
    }
}

fn is_strong_entity_tag(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('"') && value.ends_with('"') && !value.starts_with("W/")
}

fn choose_piece_size(total_size: u64, connections: u8) -> u64 {
    let desired_pieces = u64::from(connections).saturating_mul(4).max(1);
    let raw = total_size.div_ceil(desired_pieces);
    let aligned = raw.div_ceil(64 * 1024) * 64 * 1024;
    aligned.clamp(MIN_PIECE_SIZE, MAX_PIECE_SIZE)
}

fn build_pieces(total_size: u64, piece_size: u64) -> Vec<Piece> {
    if total_size == 0 {
        return Vec::new();
    }
    (0..total_size.div_ceil(piece_size))
        .map(|index| {
            let start = index * piece_size;
            Piece {
                index: index as usize,
                start,
                end: (start + piece_size - 1).min(total_size - 1),
            }
        })
        .collect()
}

fn load_compatible_state(
    path: &Path,
    request: &DownloadRequest<'_>,
    probe: &ProbeResult,
    piece_size: u64,
    piece_count: usize,
) -> Result<Option<ResumeState>, DownloadError> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(_) => return Ok(None),
    };
    let state: ResumeState = match serde_json::from_slice(&contents) {
        Ok(state) => state,
        Err(_) => return Ok(None),
    };
    let validator_matches = state.etag == probe.etag && state.last_modified == probe.last_modified;
    let matches = state.schema_version == 1
        && state.url == request.url
        && state.identity == request.identity
        && state.total_size == probe.total_size
        && state.piece_size == piece_size
        && state.completed.len() == piece_count
        && validator_matches;
    Ok(matches.then_some(state))
}

fn save_state(path: &Path, state: &ResumeState) -> Result<(), DownloadError> {
    let parent = path
        .parent()
        .ok_or_else(|| DownloadError::State("状态路径缺少父目录".to_string()))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|source| io_error("创建断点状态文件", path, source))?;
    serde_json::to_writer(temporary.as_file_mut(), state)
        .map_err(|error| DownloadError::State(error.to_string()))?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|source| io_error("同步断点状态文件", path, source))?;
    temporary
        .persist(path)
        .map_err(|error| io_error("保存断点状态文件", path, error.error))?;
    Ok(())
}

fn resume_path(temporary: &Path) -> PathBuf {
    let mut value = temporary.as_os_str().to_os_string();
    value.push(".nacl.json");
    PathBuf::from(value)
}

fn header_string(response: &Response, name: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn record_first_error(
    first_error: &Mutex<Option<DownloadError>>,
    stop: &AtomicBool,
    error: DownloadError,
) {
    let mut first = first_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if first.is_none() {
        *first = Some(error);
    }
    stop.store(true, Ordering::Release);
}

fn wait_for_control(control: &(impl Fn() -> DownloadControl + Sync)) -> Result<(), DownloadError> {
    loop {
        match control() {
            DownloadControl::Running => return Ok(()),
            DownloadControl::Cancelled => return Err(DownloadError::Cancelled),
            DownloadControl::Paused => thread::sleep(Duration::from_millis(100)),
        }
    }
}

struct TransferRate {
    window_started: Instant,
    window_bytes: u64,
    last_speed: u64,
}

impl TransferRate {
    fn new() -> Self {
        Self {
            window_started: Instant::now(),
            window_bytes: 0,
            last_speed: 0,
        }
    }

    fn record(&mut self, bytes: u64) -> u64 {
        self.window_bytes = self.window_bytes.saturating_add(bytes);
        let elapsed = self.window_started.elapsed();
        if elapsed >= Duration::from_millis(250) {
            self.last_speed = (self.window_bytes as f64 / elapsed.as_secs_f64()) as u64;
            self.window_started = Instant::now();
            self.window_bytes = 0;
        }
        self.last_speed
    }
}

struct RateLimiter {
    bytes_per_second: u64,
    next_slot: Mutex<Instant>,
}

impl RateLimiter {
    fn new(limit_kib_per_second: u32) -> Self {
        Self {
            bytes_per_second: u64::from(limit_kib_per_second) * 1024,
            next_slot: Mutex::new(Instant::now()),
        }
    }

    fn throttle(&self, bytes: u64) {
        if self.bytes_per_second == 0 {
            return;
        }
        let duration = Duration::from_secs_f64(bytes as f64 / self.bytes_per_second as f64);
        let target = {
            let mut next = self
                .next_slot
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let now = Instant::now();
            if *next < now {
                *next = now;
            }
            *next += duration;
            *next
        };
        if let Some(delay) = target.checked_duration_since(Instant::now()) {
            thread::sleep(delay);
        }
    }
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> DownloadError {
    DownloadError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(windows)]
fn write_all_at(file: &File, mut buffer: &[u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buffer.is_empty() {
        let count = file.seek_write(buffer, offset)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "分片写入返回 0 字节",
            ));
        }
        buffer = &buffer[count..];
        offset += count as u64;
    }
    Ok(())
}

#[cfg(unix)]
fn write_all_at(file: &File, mut buffer: &[u8], mut offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    while !buffer.is_empty() {
        let count = file.write_at(buffer, offset)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "分片写入返回 0 字节",
            ));
        }
        buffer = &buffer[count..];
        offset += count as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_pieces, choose_piece_size, download, is_strong_entity_tag, parse_content_range,
        save_state, segmented_download, DownloadControl, DownloadEngine, DownloadOptions,
        DownloadRequest, ResumeState,
    };
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
    use std::thread;
    use std::time::Duration;

    #[derive(Clone, Copy)]
    enum ServerMode {
        Range,
        RangeWithChangedEtag,
        DisconnectFirstPiece,
        IgnoreRange,
        MalformedRange,
    }

    struct TestServer {
        url: String,
        address: SocketAddr,
        stop: Arc<AtomicBool>,
        requested_ranges: Arc<Mutex<Vec<String>>>,
        join: Option<thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn start(data: Vec<u8>, mode: ServerMode) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let requested_ranges = Arc::new(Mutex::new(Vec::new()));
            let worker_stop = Arc::clone(&stop);
            let worker_ranges = Arc::clone(&requested_ranges);
            let disconnected = Arc::new(AtomicBool::new(false));
            let data = Arc::new(data);
            let join = thread::spawn(move || {
                let mut handlers = Vec::new();
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if worker_stop.load(Ordering::Acquire) {
                                break;
                            }
                            let data = Arc::clone(&data);
                            let ranges = Arc::clone(&worker_ranges);
                            let disconnected = Arc::clone(&disconnected);
                            handlers.push(thread::spawn(move || {
                                serve_connection(stream, &data, mode, &ranges, &disconnected)
                            }));
                        }
                        Err(_) if worker_stop.load(Ordering::Acquire) => break,
                        Err(_) => continue,
                    }
                }
                for handler in handlers {
                    let _ = handler.join();
                }
            });
            Self {
                url: format!("http://{address}/file.bin"),
                address,
                stop,
                requested_ranges,
                join: Some(join),
            }
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            let _ = TcpStream::connect(self.address);
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    fn serve_connection(
        mut stream: TcpStream,
        data: &[u8],
        mode: ServerMode,
        requested_ranges: &Mutex<Vec<String>>,
        disconnected: &AtomicBool,
    ) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let Ok(count) = stream.read(&mut buffer) else {
                return;
            };
            if count == 0 {
                return;
            }
            request.extend_from_slice(&buffer[..count]);
        }
        let request = String::from_utf8_lossy(&request);
        let range = request.lines().find_map(|line| {
            line.strip_prefix("Range: ")
                .or_else(|| line.strip_prefix("range: "))
                .map(str::trim)
        });
        if let Some(range) = range {
            requested_ranges.lock().unwrap().push(range.to_string());
        }
        match (mode, range) {
            (
                ServerMode::Range
                | ServerMode::RangeWithChangedEtag
                | ServerMode::DisconnectFirstPiece,
                Some(range),
            ) => {
                let (start, end) = parse_request_range(range, data.len() as u64);
                let etag = if matches!(mode, ServerMode::RangeWithChangedEtag) {
                    "\"nacl-test-v2\""
                } else {
                    "\"nacl-test-v1\""
                };
                if matches!(mode, ServerMode::DisconnectFirstPiece)
                    && range != "bytes=0-0"
                    && !disconnected.swap(true, Ordering::AcqRel)
                {
                    let full = &data[start as usize..=end as usize];
                    let header = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nETag: {etag}\r\nConnection: close\r\n\r\n",
                        full.len(),
                        data.len()
                    );
                    stream.write_all(header.as_bytes()).unwrap();
                    stream.write_all(&full[..full.len() / 2]).unwrap();
                    return;
                }
                send_response(
                    &mut stream,
                    "206 Partial Content",
                    &[
                        (
                            "Content-Range",
                            format!("bytes {start}-{end}/{}", data.len()),
                        ),
                        ("ETag", etag.to_string()),
                    ],
                    &data[start as usize..=end as usize],
                );
            }
            (ServerMode::MalformedRange, Some(_)) => send_response(
                &mut stream,
                "206 Partial Content",
                &[("Content-Range", format!("bytes 1-1/{}", data.len()))],
                &data[1..2],
            ),
            _ => send_response(&mut stream, "200 OK", &[], data),
        }
    }

    fn parse_request_range(value: &str, total: u64) -> (u64, u64) {
        let value = value.strip_prefix("bytes=").unwrap();
        let (start, end) = value.split_once('-').unwrap();
        let start = start.parse::<u64>().unwrap();
        let end = end.parse::<u64>().unwrap_or(total - 1).min(total - 1);
        (start, end)
    }

    fn send_response(
        stream: &mut TcpStream,
        status: &str,
        headers: &[(&str, String)],
        body: &[u8],
    ) {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            response.push_str(&format!("{name}: {value}\r\n"));
        }
        response.push_str("\r\n");
        if stream.write_all(response.as_bytes()).is_err() {
            return;
        }
        let _ = stream.write_all(body);
    }

    fn test_data(size: usize) -> Vec<u8> {
        (0..size).map(|index| (index % 251) as u8).collect()
    }

    fn test_options() -> DownloadOptions {
        DownloadOptions {
            segment_connections: 4,
            segment_threshold_bytes: 1024,
            retry_count: 2,
            speed_limit_kib_per_second: 0,
        }
    }

    fn test_client() -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(0)
            .build()
            .unwrap()
    }

    fn network_test_guard() -> MutexGuard<'static, ()> {
        static NETWORK_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        NETWORK_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn pieces_cover_file_without_gaps_or_overlap() {
        for total in [1, 1024, 2 * 1024 * 1024, 17 * 1024 * 1024 + 37] {
            let size = choose_piece_size(total, 4);
            let pieces = build_pieces(total, size);
            assert_eq!(pieces.first().map(|piece| piece.start), Some(0));
            assert_eq!(pieces.last().map(|piece| piece.end), Some(total - 1));
            for pair in pieces.windows(2) {
                assert_eq!(pair[0].end + 1, pair[1].start);
            }
            assert_eq!(pieces.iter().map(|piece| piece.len()).sum::<u64>(), total);
        }
    }

    #[test]
    fn parses_strict_content_range() {
        assert_eq!(
            parse_content_range("bytes 10-19/100").unwrap(),
            (10, 19, 100)
        );
        assert!(parse_content_range("bytes */100").is_err());
        assert!(parse_content_range("bytes 20-10/100").is_err());
        assert!(parse_content_range("items 0-1/2").is_err());
    }

    #[test]
    fn accepts_only_strong_quoted_etags_for_if_range() {
        assert!(is_strong_entity_tag("\"strong-v1\""));
        assert!(!is_strong_entity_tag("W/\"weak-v1\""));
        assert!(!is_strong_entity_tag("0x8DAB7EC3FD88ECD"));
    }

    #[test]
    fn segmented_download_writes_exact_file() {
        let _guard = network_test_guard();
        let data = test_data(5 * 1024 * 1024 + 37);
        let server = TestServer::start(data.clone(), ServerMode::Range);
        let directory = tempfile::tempdir().unwrap();
        let temporary = directory.path().join("download.part");
        segmented_download(
            &test_client(),
            &DownloadRequest {
                label: "test",
                url: &server.url,
                expected_size: data.len() as u64,
                identity: "test-v1",
                temporary_path: &temporary,
            },
            test_options(),
            &|| DownloadControl::Running,
            &|_| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(temporary).unwrap(), data);
    }

    #[test]
    fn ignored_range_falls_back_to_streaming() {
        let _guard = network_test_guard();
        let data = test_data(1024 * 1024 + 19);
        let server = TestServer::start(data.clone(), ServerMode::IgnoreRange);
        let directory = tempfile::tempdir().unwrap();
        let temporary = directory.path().join("download.part");
        let outcome = download(
            &test_client(),
            DownloadRequest {
                label: "fallback",
                url: &server.url,
                expected_size: data.len() as u64,
                identity: "fallback-v1",
                temporary_path: &temporary,
            },
            test_options(),
            &|| DownloadControl::Running,
            &|_| {},
        )
        .unwrap();
        assert_eq!(outcome.engine, DownloadEngine::Streaming);
        assert_eq!(std::fs::read(temporary).unwrap(), data);
    }

    #[test]
    fn malformed_range_falls_back_to_streaming() {
        let _guard = network_test_guard();
        let data = test_data(1024 * 1024 + 31);
        let server = TestServer::start(data.clone(), ServerMode::MalformedRange);
        let directory = tempfile::tempdir().unwrap();
        let temporary = directory.path().join("download.part");
        let outcome = download(
            &test_client(),
            DownloadRequest {
                label: "malformed",
                url: &server.url,
                expected_size: data.len() as u64,
                identity: "malformed-v1",
                temporary_path: &temporary,
            },
            test_options(),
            &|| DownloadControl::Running,
            &|_| {},
        )
        .unwrap();
        assert_eq!(outcome.engine, DownloadEngine::Streaming);
        assert_eq!(std::fs::read(temporary).unwrap(), data);
    }

    #[test]
    fn resumes_only_incomplete_pieces() {
        let _guard = network_test_guard();
        let data = test_data(5 * 1024 * 1024 + 7);
        let server = TestServer::start(data.clone(), ServerMode::Range);
        let directory = tempfile::tempdir().unwrap();
        let temporary = directory.path().join("download.part");
        let piece_size = choose_piece_size(data.len() as u64, 4);
        let pieces = build_pieces(data.len() as u64, piece_size);
        let file = std::fs::File::create(&temporary).unwrap();
        file.set_len(data.len() as u64).unwrap();
        super::write_all_at(&file, &data[..piece_size as usize], 0).unwrap();
        save_state(
            &super::resume_path(&temporary),
            &ResumeState {
                schema_version: 1,
                url: server.url.clone(),
                identity: "resume-v1".to_string(),
                total_size: data.len() as u64,
                piece_size,
                completed: pieces.iter().map(|piece| piece.index == 0).collect(),
                etag: Some("\"nacl-test-v1\"".to_string()),
                last_modified: None,
            },
        )
        .unwrap();

        segmented_download(
            &test_client(),
            &DownloadRequest {
                label: "resume",
                url: &server.url,
                expected_size: data.len() as u64,
                identity: "resume-v1",
                temporary_path: &temporary,
            },
            test_options(),
            &|| DownloadControl::Running,
            &|_| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(temporary).unwrap(), data);
        let ranges = server.requested_ranges.lock().unwrap();
        assert!(!ranges
            .iter()
            .any(|range| range == &format!("bytes=0-{}", piece_size - 1)));
    }

    #[test]
    fn cancellation_stops_without_streaming_fallback() {
        let _guard = network_test_guard();
        let data = test_data(3 * 1024 * 1024);
        let server = TestServer::start(data, ServerMode::Range);
        let directory = tempfile::tempdir().unwrap();
        let temporary = directory.path().join("download.part");
        let cancelled = AtomicBool::new(false);
        let result = download(
            &test_client(),
            DownloadRequest {
                label: "cancel",
                url: &server.url,
                expected_size: 3 * 1024 * 1024,
                identity: "cancel-v1",
                temporary_path: &temporary,
            },
            test_options(),
            &|| {
                if cancelled.load(Ordering::Acquire) {
                    DownloadControl::Cancelled
                } else {
                    DownloadControl::Running
                }
            },
            &|update| {
                if update.downloaded_bytes > 0 {
                    cancelled.store(true, Ordering::Release);
                }
            },
        );
        assert!(matches!(result, Err(super::DownloadError::Cancelled)));
    }

    #[test]
    fn retries_piece_after_connection_breaks_mid_transfer() {
        let _guard = network_test_guard();
        let data = test_data(5 * 1024 * 1024 + 101);
        let server = TestServer::start(data.clone(), ServerMode::DisconnectFirstPiece);
        let directory = tempfile::tempdir().unwrap();
        let temporary = directory.path().join("download.part");
        segmented_download(
            &test_client(),
            &DownloadRequest {
                label: "disconnect",
                url: &server.url,
                expected_size: data.len() as u64,
                identity: "disconnect-v1",
                temporary_path: &temporary,
            },
            test_options(),
            &|| DownloadControl::Running,
            &|_| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(temporary).unwrap(), data);
    }

    #[test]
    fn changed_etag_invalidates_completed_piece_state() {
        let _guard = network_test_guard();
        let data = test_data(5 * 1024 * 1024 + 13);
        let server = TestServer::start(data.clone(), ServerMode::RangeWithChangedEtag);
        let directory = tempfile::tempdir().unwrap();
        let temporary = directory.path().join("download.part");
        let piece_size = choose_piece_size(data.len() as u64, 4);
        let pieces = build_pieces(data.len() as u64, piece_size);
        let file = std::fs::File::create(&temporary).unwrap();
        file.set_len(data.len() as u64).unwrap();
        super::write_all_at(&file, &vec![0; piece_size as usize], 0).unwrap();
        save_state(
            &super::resume_path(&temporary),
            &ResumeState {
                schema_version: 1,
                url: server.url.clone(),
                identity: "etag-v1".to_string(),
                total_size: data.len() as u64,
                piece_size,
                completed: pieces.iter().map(|piece| piece.index == 0).collect(),
                etag: Some("\"nacl-test-v1\"".to_string()),
                last_modified: None,
            },
        )
        .unwrap();
        segmented_download(
            &test_client(),
            &DownloadRequest {
                label: "etag",
                url: &server.url,
                expected_size: data.len() as u64,
                identity: "etag-v1",
                temporary_path: &temporary,
            },
            test_options(),
            &|| DownloadControl::Running,
            &|_| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(temporary).unwrap(), data);
    }
}
