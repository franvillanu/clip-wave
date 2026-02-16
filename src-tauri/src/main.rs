// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use tauri::{Emitter, Manager};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::io::ErrorKind;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::{env, fs};

#[cfg(windows)]
fn apply_no_window(cmd: &mut Command) {
  use std::os::windows::process::CommandExt;
  // CREATE_NO_WINDOW
  cmd.creation_flags(0x08000000);
}

#[cfg(not(windows))]
fn apply_no_window(_cmd: &mut Command) {}

#[derive(Clone, Debug, Serialize)]
struct AudioStreamInfo {
  // 0-based order within audio streams (used for ffmpeg mapping: `0:a:{order}`).
  order: i32,
  // Original ffprobe stream index (global), when available; -1 for in-process probes.
  index: i32,
  codec_name: String,
  channels: Option<i32>,
  language: String,
  title: String,
}

#[derive(Clone, Debug, Serialize)]
struct SubtitleStreamInfo {
  // 0-based order within subtitle streams (future: used for `0:s:{order}`).
  order: i32,
  index: i32,
  codec_name: String,
  language: String,
  title: String,
}

#[derive(Debug, Serialize)]
struct ProbeResult {
  input_path: String,
  duration_seconds: Option<f64>,
  audio_streams: Vec<AudioStreamInfo>,
  subtitle_streams: Vec<SubtitleStreamInfo>,
  ffmpeg_bin_dir_used: String,
  ffprobe_path: String,
  ffprobe_args: Vec<String>,
  ffprobe_runner: String,
  cwd: String,
  timing_ms: ProbeTimingInfo,
}

#[derive(Debug, Serialize)]
struct ProbeTimingInfo {
  validation_ms: f64,
  resolve_binaries_ms: f64,
  ffprobe_spawn_ms: f64,
  ffprobe_first_stdout_byte_ms: Option<f64>,
  ffprobe_first_stderr_byte_ms: Option<f64>,
  ffprobe_execution_ms: f64,
  ffprobe_wait_ms: f64,
  json_parsing_ms: f64,
  total_ms: f64,
  cache_hit: bool,
}

#[derive(Debug, Serialize)]
struct TrimResult {
  output_path: String,
  requested_duration_seconds: f64,
  actual_duration_seconds: Option<f64>,
  duration_warning: Option<String>,
}

#[derive(Debug, Serialize)]
struct FfmpegCheckResult {
  ok: bool,
  message: String,
  ffmpeg_bin_dir_used: String,
}

#[derive(Debug, Serialize)]
struct WingetStatusResult {
  available: bool,
  message: String,
}

#[derive(Debug, Serialize)]
struct LosslessPreflightResult {
  in_time_seconds: f64,
  nearest_keyframe_seconds: Option<f64>,
  next_keyframe_seconds: Option<f64>,
  start_shift_seconds: Option<f64>,
  // OUT point analysis
  out_time_seconds: Option<f64>,
  out_prev_keyframe_seconds: Option<f64>,
  out_next_keyframe_seconds: Option<f64>,
  end_shift_seconds: Option<f64>,
}

#[derive(Debug, Serialize)]
struct WarmupResult {
  ffprobe_path: String,
  ffprobe_runner: String,
  ms: f64,
}

#[derive(Debug, Serialize)]
struct SpawnDebugInfo {
  phase: String,
  program: String,
  args: Vec<String>,
  cwd: String,
  program_exists: bool,
  exit_code: Option<i32>,
  success: bool,
  stdout_len: usize,
  stderr_len: usize,
  stderr_head: String,
}

#[derive(Debug, Serialize)]
struct DurationProbeResult {
  input_path: String,
  duration_seconds: Option<f64>,
  ffmpeg_bin_dir_used: String,
  ffprobe_path: String,
  ffprobe_args: Vec<String>,
  ffprobe_runner: String,
  cwd: String,
  timing_ms: ProbeTimingInfo,
  debug: Option<SpawnDebugInfo>,
}

#[derive(Debug, Serialize)]
struct TracksProbeTimingInfo {
  validation_ms: f64,
  resolve_binaries_ms: f64,
  audio_ffprobe_ms: f64,
  subs_ffprobe_ms: f64,
  total_ms: f64,
  cache_hit: bool,
}

#[derive(Debug, Serialize)]
struct TracksProbeResult {
  input_path: String,
  audio_streams: Vec<AudioStreamInfo>,
  subtitle_streams: Vec<SubtitleStreamInfo>,
  ffmpeg_bin_dir_used: String,
  ffprobe_path: String,
  ffprobe_runner: String,
  cwd: String,
  timing_ms: TracksProbeTimingInfo,
  debug: Vec<SpawnDebugInfo>,
}

#[derive(Debug, Serialize)]
struct SubtitlesProbeTimingInfo {
  validation_ms: f64,
  resolve_binaries_ms: f64,
  ffprobe_ms: f64,
  total_ms: f64,
  cache_hit: bool,
}

#[derive(Debug, Serialize)]
struct SubtitlesProbeResult {
  input_path: String,
  subtitle_streams: Vec<SubtitleStreamInfo>,
  ffmpeg_bin_dir_used: String,
  ffprobe_path: String,
  ffprobe_runner: String,
  cwd: String,
  timing_ms: SubtitlesProbeTimingInfo,
  debug: Option<SpawnDebugInfo>,
}

#[allow(dead_code)]
fn parse_hh_mm_ss(input: &str) -> Result<u64, String> {
  // Parse with milliseconds support and round down to whole seconds
  let seconds_f64 = parse_hh_mm_ss_with_millis(input)?;
  Ok(seconds_f64.floor() as u64)
}

// Parse time in format hh:mm:ss or hh:mm:ss.milliseconds
fn parse_hh_mm_ss_with_millis(input: &str) -> Result<f64, String> {
  let parts: Vec<&str> = input.split(':').collect();
  if parts.len() != 3 {
    return Err("Time must be in format hh:mm:ss or hh:mm:ss.milliseconds".to_string());
  }

  let (h, m, s_with_millis) = (parts[0], parts[1], parts[2]);

  // Validate hours (any number of digits)
  if h.is_empty() || !h.chars().all(|c| c.is_ascii_digit()) {
    return Err("Invalid hours".to_string());
  }

  // Validate minutes (must be exactly 2 digits)
  if m.len() != 2 || !m.chars().all(|c| c.is_ascii_digit()) {
    return Err("Invalid minutes (must be 2 digits)".to_string());
  }

  // Parse seconds part (may include decimal point for milliseconds)
  let seconds_f64: f64 = s_with_millis.parse().map_err(|_| "Invalid seconds".to_string())?;

  let hours: u64 = h.parse().map_err(|_| "Invalid hours".to_string())?;
  let minutes: u64 = m.parse().map_err(|_| "Invalid minutes".to_string())?;

  if minutes >= 60 || seconds_f64 >= 60.0 || seconds_f64 < 0.0 {
    return Err("Minutes and seconds must be < 60".to_string());
  }

  Ok(hours as f64 * 3600.0 + minutes as f64 * 60.0 + seconds_f64)
}

fn time_for_filename(input: &str) -> String {
  input.replace(':', "h")
}

fn winget_windowsapps_stub_path() -> Option<PathBuf> {
  let local_app_data = env::var_os("LOCALAPPDATA")?;
  let p = PathBuf::from(local_app_data)
    .join("Microsoft")
    .join("WindowsApps")
    .join("winget.exe");
  if p.is_file() { Some(p) } else { None }
}

fn resolve_ffmpeg_binaries(ffmpeg_bin_dir: &str) -> (PathBuf, PathBuf) {
  let dir_str = ffmpeg_bin_dir.trim();
  if dir_str.is_empty() {
    return (PathBuf::from("ffmpeg"), PathBuf::from("ffprobe"));
  }
  let dir = PathBuf::from(dir_str);
  (dir.join("ffmpeg.exe"), dir.join("ffprobe.exe"))
}

fn looks_like_ffmpeg_bin_dir(dir: &Path) -> bool {
  dir.join("ffmpeg.exe").is_file() && dir.join("ffprobe.exe").is_file()
}

fn auto_detect_ffmpeg_bin_dir() -> Option<PathBuf> {
  // PRIORITY 1: Check bundled location first (fastest, most reliable)
  if let Some(bundled) = get_bundled_ffmpeg_dir() {
    return Some(bundled);
  }

  // PRIORITY 2: Check environment variables
  for key in ["VIDEO_TRIM_FFMPEG_BIN_DIR", "FFMPEG_BIN_DIR"] {
    if let Ok(v) = env::var(key) {
      let p = PathBuf::from(v);
      if looks_like_ffmpeg_bin_dir(&p) {
        return Some(p);
      }
    }
  }

  let mut roots = Vec::new();

  // PRIORITY 3: Exe directory
  if let Ok(exe) = env::current_exe() {
    if let Some(exe_dir) = exe.parent() {
      roots.push(exe_dir.to_path_buf());
      if let Some(parent) = exe_dir.parent() {
        roots.push(parent.to_path_buf());
      }
    }
  }

  // Skip current_dir() to avoid C:\Windows\System32 when launched from Start Menu

  roots.sort();
  roots.dedup();

  for root in roots {
    // Quick check: <root>/bin first (most common)
    let bin = root.join("bin");
    if looks_like_ffmpeg_bin_dir(&bin) {
      return Some(bin);
    }

    // Only scan directory if quick check failed - limit to first 20 entries
    if let Ok(entries) = fs::read_dir(&root) {
      for (i, entry) in entries.flatten().enumerate() {
        if i >= 20 {
          break; // Limit scanning to avoid long delays
        }

        let path = entry.path();
        if !path.is_dir() {
          continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("ffmpeg-") && name.contains("essentials_build") {
          let bin = path.join("bin");
          if looks_like_ffmpeg_bin_dir(&bin) {
            return Some(bin);
          }
        }
      }
    }
  }

  None
}

fn resolve_ffmpeg_binaries_with_fallback(ffmpeg_bin_dir: &str) -> (PathBuf, PathBuf, String) {
  let dir_str = ffmpeg_bin_dir.trim();
  if !dir_str.is_empty() {
    let (ffmpeg, ffprobe) = resolve_ffmpeg_binaries(dir_str);
    return (ffmpeg, ffprobe, dir_str.to_string());
  }

  if let Some(dir) = auto_detect_ffmpeg_bin_dir() {
    let dir_string = dir.to_string_lossy().to_string();
    let (ffmpeg, ffprobe) = resolve_ffmpeg_binaries(&dir_string);
    return (ffmpeg, ffprobe, dir_string);
  }

  (PathBuf::from("ffmpeg"), PathBuf::from("ffprobe"), String::new())
}

fn validate_ffmpeg_bin_dir(ffmpeg_bin_dir: &str) -> Result<(), String> {
  let dir_str = ffmpeg_bin_dir.trim();
  if dir_str.is_empty() {
    return Ok(());
  }

  let dir = Path::new(dir_str);
  if !dir.exists() {
    return Err("FFmpeg bin folder does not exist".to_string());
  }
  if !dir.is_dir() {
    return Err("FFmpeg bin folder is not a directory".to_string());
  }

  let ffmpeg = dir.join("ffmpeg.exe");
  let ffprobe = dir.join("ffprobe.exe");
  if !ffmpeg.is_file() {
    return Err("FFmpeg bin folder must contain ffmpeg.exe".to_string());
  }
  if !ffprobe.is_file() {
    return Err("FFmpeg bin folder must contain ffprobe.exe".to_string());
  }

  Ok(())
}

fn normalize_rotation_degrees(deg: i32) -> i32 {
  let mut d = deg % 360;
  if d < 0 {
    d += 360;
  }
  match d {
    0 | 90 | 180 | 270 => d,
    _ => 0,
  }
}

fn rotation_filter_for_degrees(deg: i32) -> Option<&'static str> {
  match normalize_rotation_degrees(deg) {
    // Many sources (especially phone videos) report rotation as counter-clockwise degrees.
    // FFmpeg's transpose=2 is 90° CCW; transpose=1 is 90° CW.
    90 => Some("transpose=2"),
    180 => Some("hflip,vflip"),
    270 => Some("transpose=1"),
    _ => None,
  }
}

fn probe_video_rotation_degrees_best_effort(ffprobe_path: &Path, input_path: &str) -> i32 {
  let mut cmd = Command::new(ffprobe_path);
  apply_no_window(&mut cmd);
  let output = cmd
    .args([
      "-v",
      "quiet",
      "-print_format",
      "json",
      "-select_streams",
      "v:0",
      "-show_streams",
    ])
    .arg(input_path)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .output();

  let Ok(output) = output else {
    return 0;
  };
  if !output.status.success() {
    return 0;
  }

  let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
    return 0;
  };

  let Some(video) = json
    .get("streams")
    .and_then(|s| s.as_array())
    .and_then(|arr| arr.first())
  else {
    return 0;
  };

  if let Some(tags) = video.get("tags").and_then(|t| t.as_object()) {
    if let Some(deg) = tags
      .get("rotate")
      .or_else(|| tags.get("Rotate"))
      .and_then(|v| v.as_str())
      .and_then(|s| s.parse::<i32>().ok())
    {
      return normalize_rotation_degrees(deg);
    }
  }

  if let Some(side) = video.get("side_data_list").and_then(|v| v.as_array()) {
    for item in side {
      if let Some(deg) = item
        .get("rotation")
        .and_then(|v| v.as_f64())
        .map(|v| v.round() as i32)
      {
        let d = normalize_rotation_degrees(deg);
        if d != 0 {
          return d;
        }
      }

      if let Some(deg) = item
        .get("rotation")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i32>().ok())
      {
        let d = normalize_rotation_degrees(deg);
        if d != 0 {
          return d;
        }
      }
    }
  }

  0
}

fn ensure_input_file_exists(input_path: &str) -> Result<(), String> {
  let p = Path::new(input_path);
  if !p.exists() {
    return Err("Input file does not exist".to_string());
  }
  if !p.is_file() {
    return Err("Input path is not a file".to_string());
  }
  Ok(())
}

fn normalize_input_path_for_cli(input_path: &str) -> String {
  // Some frontends may pass file URLs. Convert best-effort to a local path.
  let s = input_path.trim();
  let lower = s.to_ascii_lowercase();
  if !(lower.starts_with("file://")) {
    return s.to_string();
  }

  let mut rest = if lower.starts_with("file:///") {
    &s[8..]
  } else {
    &s[7..]
  };

  // Drop localhost authority if present.
  let lower_rest = rest.to_ascii_lowercase();
  if lower_rest.starts_with("localhost/") {
    rest = &rest[10..];
  }

  let bytes = rest.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'%' if i + 2 < bytes.len() => {
        let h1 = bytes[i + 1];
        let h2 = bytes[i + 2];
        let hex = |b: u8| -> Option<u8> {
          match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
          }
        };
        if let (Some(a), Some(b)) = (hex(h1), hex(h2)) {
          out.push((a << 4) | b);
          i += 3;
          continue;
        }
        out.push(bytes[i]);
        i += 1;
      }
      b'/' => {
        out.push(b'\\');
        i += 1;
      }
      c => {
        out.push(c);
        i += 1;
      }
    }
  }

  String::from_utf8_lossy(&out).to_string()
}

// Note: ffprobe is always run directly (not via PowerShell) to avoid shell startup overhead and differences.

fn stable_working_dir() -> Option<PathBuf> {
  if let Ok(exe) = env::current_exe() {
    if let Some(dir) = exe.parent() {
      return Some(dir.to_path_buf());
    }
  }

  if let Ok(profile) = env::var("USERPROFILE") {
    let p = PathBuf::from(profile);
    if p.is_dir() {
      return Some(p);
    }
  }

  let tmp = env::temp_dir();
  if tmp.is_dir() {
    return Some(tmp);
  }

  None
}

#[cfg(windows)]
mod win_mf {
  use super::AudioStreamInfo;
  use std::sync::OnceLock;
  use windows::core::GUID;
  use windows::core::PCWSTR;
  use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
  use windows::Win32::Media::MediaFoundation::{
    MFCreateSourceReaderFromURL, MFStartup, MF_VERSION, IMFMediaType, IMFSourceReader, MF_E_INVALIDSTREAMNUMBER,
    MF_MT_AUDIO_NUM_CHANNELS, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_PD_DURATION, MF_SOURCE_READER_MEDIASOURCE,
    MFMediaType_Audio, MFAudioFormat_AAC, MFAudioFormat_ALAC, MFAudioFormat_Dolby_AC3, MFAudioFormat_Dolby_DDPlus,
    MFAudioFormat_FLAC, MFAudioFormat_MP3, MFAudioFormat_PCM,
  };
  use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

  fn mf_startup_once() -> Result<(), String> {
    static STARTED: OnceLock<Result<(), String>> = OnceLock::new();
    STARTED
      .get_or_init(|| unsafe {
        MFStartup(MF_VERSION, 0).map_err(|e| format!("MFStartup failed: {e}"))
      })
      .clone()
  }

  fn co_init_best_effort() -> Result<(), String> {
    unsafe {
      let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
      if hr == RPC_E_CHANGED_MODE {
        return Ok(());
      }
      hr.ok().map_err(|e| format!("CoInitializeEx failed: {e}"))
    }
  }

  fn to_wide_null_terminated(s: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    wide.push(0);
    wide
  }

  fn audio_codec_name_from_subtype(subtype: &GUID) -> String {
    if *subtype == MFAudioFormat_AAC {
      return "aac".to_string();
    }
    if *subtype == MFAudioFormat_MP3 {
      return "mp3".to_string();
    }
    if *subtype == MFAudioFormat_Dolby_AC3 {
      return "ac3".to_string();
    }
    if *subtype == MFAudioFormat_Dolby_DDPlus {
      return "eac3".to_string();
    }
    if *subtype == MFAudioFormat_FLAC {
      return "flac".to_string();
    }
    if *subtype == MFAudioFormat_ALAC {
      return "alac".to_string();
    }
    if *subtype == MFAudioFormat_PCM {
      return "pcm".to_string();
    }
    format!("{subtype:?}")
  }

  fn get_duration_seconds(reader: &IMFSourceReader) -> Result<Option<f64>, String> {
    let var = unsafe {
      reader
        .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
        .map_err(|e| format!("MF GetPresentationAttribute(duration) failed: {e}"))?
    };
    let duration_100ns = u64::try_from(&var).ok();
    Ok(duration_100ns.map(|v| v as f64 / 10_000_000.0))
  }

  fn open_reader(input_path: &str) -> Result<IMFSourceReader, String> {
    co_init_best_effort()?;
    mf_startup_once()?;

    let wide = to_wide_null_terminated(input_path);
    let url = PCWSTR::from_raw(wide.as_ptr());

    unsafe { MFCreateSourceReaderFromURL(url, None).map_err(|e| format!("MFCreateSourceReaderFromURL failed: {e}")) }
  }

  fn try_get_stream_language(reader: &IMFSourceReader, stream_index: u32) -> Option<String> {
    // MF_SD_LANGUAGE isn't consistently available across containers; ignore failures.
    let guid = windows::Win32::Media::MediaFoundation::MF_SD_LANGUAGE;
    let var = unsafe { reader.GetPresentationAttribute(stream_index, &guid).ok()? };
    let text = var.to_string();
    if text.is_empty() { None } else { Some(text) }
  }

  fn media_type_major(mt: &IMFMediaType) -> Result<GUID, String> {
    unsafe {
      mt.GetGUID(&MF_MT_MAJOR_TYPE)
        .map_err(|e| format!("MF GetGUID(major) failed: {e}"))
    }
  }

  fn media_type_subtype(mt: &IMFMediaType) -> Option<GUID> {
    unsafe { mt.GetGUID(&MF_MT_SUBTYPE).ok() }
  }

  fn media_type_audio_channels(mt: &IMFMediaType) -> Option<i32> {
    unsafe { mt.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).ok().map(|v| v as i32) }
  }

  pub fn probe_duration_seconds(input_path: &str) -> Result<Option<f64>, String> {
    let reader = open_reader(input_path)?;
    get_duration_seconds(&reader)
  }

  pub fn probe_audio_streams(input_path: &str) -> Result<Vec<AudioStreamInfo>, String> {
    let reader = open_reader(input_path)?;
    let mut audio_streams = Vec::new();
    let mut audio_order: i32 = 0;

    // Enumerate streams until MF says the stream number is invalid.
    for stream_index in 0u32..128u32 {
      let mt = match unsafe { reader.GetNativeMediaType(stream_index, 0) } {
        Ok(mt) => mt,
        Err(e) => {
          if e.code() == MF_E_INVALIDSTREAMNUMBER {
            break;
          }
          continue;
        }
      };
      let major = media_type_major(&mt)?;
      if major != MFMediaType_Audio {
        continue;
      }

      let subtype = media_type_subtype(&mt);
      let codec_name = subtype.as_ref().map(audio_codec_name_from_subtype).unwrap_or_default();
      let channels = media_type_audio_channels(&mt);
      let language = try_get_stream_language(&reader, stream_index).unwrap_or_else(|| "und".to_string());

      audio_streams.push(AudioStreamInfo {
        order: audio_order,
        index: -1,
        codec_name,
        channels,
        language,
        title: String::new(),
      });
      audio_order += 1;
    }

    Ok(audio_streams)
  }

}

#[derive(Clone, Debug)]
struct CachedProbeResult {
  input_path: String,
  has_duration: bool,
  has_tracks: bool,
  has_subtitles: bool,
  duration_seconds: Option<f64>,
  audio_streams: Vec<AudioStreamInfo>,
  subtitle_streams: Vec<SubtitleStreamInfo>,
  ffmpeg_bin_dir_used: String,
  ffprobe_path: String,
  ffprobe_args: Vec<String>,
  ffprobe_runner: String,
  cwd: String,
}

fn probe_cache() -> &'static Mutex<HashMap<String, CachedProbeResult>> {
  static CACHE: OnceLock<Mutex<HashMap<String, CachedProbeResult>>> = OnceLock::new();
  CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn probe_cache_key_best_effort(input_path: &str) -> String {
  let p = Path::new(input_path);
  let meta = p.metadata();
  if let Ok(m) = meta {
    let len = m.len();
    let modified = m
      .modified()
      .ok()
      .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
      .map(|d| d.as_secs_f64())
      .unwrap_or(0.0);
    format!("{}|{}|{:.3}", input_path, len, modified)
  } else {
    input_path.to_string()
  }
}

fn parse_duration_from_ffprobe_text(stdout: &[u8]) -> Option<f64> {
  let s = String::from_utf8_lossy(stdout);
  let t = s.trim();
  if t.is_empty() {
    return None;
  }
  t.parse::<f64>().ok()
}

fn stderr_head_text(stderr: &[u8]) -> String {
  let n = std::cmp::min(stderr.len(), 200);
  String::from_utf8_lossy(&stderr[..n]).to_string()
}

fn parse_streams_from_ffprobe_json(stdout: &[u8]) -> Result<(Vec<AudioStreamInfo>, Vec<SubtitleStreamInfo>), String> {
  let json: serde_json::Value =
    serde_json::from_slice(stdout).map_err(|e| format!("Invalid ffprobe JSON: {e}"))?;

  let mut audio_streams = Vec::new();
  let mut subtitle_streams = Vec::new();

  if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
    for stream in streams {
      let index = stream
        .get("index")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "ffprobe stream missing index".to_string())? as i32;

      let codec_name = stream
        .get("codec_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

      let channels = stream
        .get("channels")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);

      let (language, title) = stream
        .get("tags")
        .and_then(|t| t.as_object())
        .map(|tags| {
          let language = tags
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("und")
            .to_string();
          let title = tags
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
          (language, title)
        })
        .unwrap_or_else(|| ("und".to_string(), "".to_string()));

      let codec_type = stream.get("codec_type").and_then(|t| t.as_str()).unwrap_or("");
      if codec_type == "audio" {
        audio_streams.push(AudioStreamInfo {
          order: 0,
          index,
          codec_name,
          channels,
          language,
          title,
        });
      } else if codec_type == "subtitle" {
        subtitle_streams.push(SubtitleStreamInfo {
          order: 0,
          index,
          codec_name,
          language,
          title,
        });
      }
    }
  }

  // Keep stable ordering and assign 0-based per-type orders for ffmpeg mapping (`0:a:{order}`, `0:s:{order}`).
  audio_streams.sort_by(|a, b| a.index.cmp(&b.index));
  for (i, s) in audio_streams.iter_mut().enumerate() {
    s.order = i as i32;
  }

  subtitle_streams.sort_by(|a, b| a.index.cmp(&b.index));
  for (i, s) in subtitle_streams.iter_mut().enumerate() {
    s.order = i as i32;
  }

  Ok((audio_streams, subtitle_streams))
}

#[tauri::command]
fn warm_ffprobe(ffmpeg_bin_dir: String) -> Result<WarmupResult, String> {
  use std::time::Instant;
  let start = Instant::now();

  validate_ffmpeg_bin_dir(&ffmpeg_bin_dir)?;
  let (_ffmpeg_path, ffprobe_path, _used) = resolve_ffmpeg_binaries_with_fallback(&ffmpeg_bin_dir);
  let ffprobe_path_text = ffprobe_path.to_string_lossy().to_string();
  let runner = "direct".to_string();
  let workdir = stable_working_dir();

  let args: Vec<String> = vec!["-version".to_string()];

  let mut c = Command::new(&ffprobe_path);
  apply_no_window(&mut c);
  c.args(&args);
  if let Some(dir) = &workdir {
    c.current_dir(dir);
  }
  let output = c
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .output()
    .map_err(|e| {
    if e.kind() == ErrorKind::NotFound {
      "Failed to run ffprobe: program not found".to_string()
    } else {
      format!("Failed to run ffprobe: {e}")
    }
    })?;

  if !output.status.success() {
    return Err("ffprobe warmup failed".to_string());
  }

  Ok(WarmupResult {
    ffprobe_path: ffprobe_path_text,
    ffprobe_runner: runner,
    ms: start.elapsed().as_secs_f64() * 1000.0,
  })
}

#[tauri::command]
fn probe_duration(input_path: String, ffmpeg_bin_dir: String) -> Result<DurationProbeResult, String> {
  use std::time::Instant;
  let start_total = Instant::now();

  let input_path = normalize_input_path_for_cli(&input_path);

  let cache_key = probe_cache_key_best_effort(&input_path);
  if let Ok(guard) = probe_cache().lock() {
    if let Some(cached) = guard.get(&cache_key).cloned() {
      if cached.has_duration {
        let timing_ms = ProbeTimingInfo {
          validation_ms: 0.0,
          resolve_binaries_ms: 0.0,
          ffprobe_spawn_ms: 0.0,
          ffprobe_first_stdout_byte_ms: None,
          ffprobe_first_stderr_byte_ms: None,
          ffprobe_execution_ms: 0.0,
          ffprobe_wait_ms: 0.0,
          json_parsing_ms: 0.0,
          total_ms: start_total.elapsed().as_secs_f64() * 1000.0,
          cache_hit: true,
        };

        return Ok(DurationProbeResult {
          input_path: cached.input_path,
          duration_seconds: cached.duration_seconds,
          ffmpeg_bin_dir_used: cached.ffmpeg_bin_dir_used,
          ffprobe_path: cached.ffprobe_path,
          ffprobe_args: cached.ffprobe_args,
          ffprobe_runner: cached.ffprobe_runner,
          cwd: cached.cwd,
          timing_ms,
          debug: None,
        });
      }
    }
  }

  let start_validation = Instant::now();
  ensure_input_file_exists(&input_path)?;
  validate_ffmpeg_bin_dir(&ffmpeg_bin_dir)?;
  let validation_ms = start_validation.elapsed().as_secs_f64() * 1000.0;

  let start_resolve = Instant::now();
  let (_ffmpeg_path, ffprobe_path, ffmpeg_bin_dir_used) =
    resolve_ffmpeg_binaries_with_fallback(&ffmpeg_bin_dir);
  let resolve_binaries_ms = start_resolve.elapsed().as_secs_f64() * 1000.0;

  let ffprobe_path_text = ffprobe_path.to_string_lossy().to_string();
  let workdir = stable_working_dir();
  let cwd_text = workdir
    .as_ref()
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_else(|| String::new());

  let ffprobe_args: Vec<String> = vec![
    "-v".to_string(),
    "error".to_string(),
    "-show_entries".to_string(),
    "format=duration".to_string(),
    "-of".to_string(),
    "default=nw=1:nk=1".to_string(),
    input_path.clone(),
  ];

  #[cfg(windows)]
  {
    let start_mf = Instant::now();
    if let Ok(duration_seconds) = win_mf::probe_duration_seconds(&input_path) {
      if duration_seconds.is_some() {
        let timing_ms = ProbeTimingInfo {
          validation_ms,
          resolve_binaries_ms,
          ffprobe_spawn_ms: 0.0,
          ffprobe_first_stdout_byte_ms: None,
          ffprobe_first_stderr_byte_ms: None,
          ffprobe_execution_ms: start_mf.elapsed().as_secs_f64() * 1000.0,
          ffprobe_wait_ms: 0.0,
          json_parsing_ms: 0.0,
          total_ms: start_total.elapsed().as_secs_f64() * 1000.0,
          cache_hit: false,
        };

        let debug = SpawnDebugInfo {
          phase: "duration_mf".to_string(),
          program: "MediaFoundation".to_string(),
          args: Vec::new(),
          cwd: cwd_text.clone(),
          program_exists: true,
          exit_code: Some(0),
          success: true,
          stdout_len: 0,
          stderr_len: 0,
          stderr_head: String::new(),
        };

        let result = DurationProbeResult {
          input_path: input_path.clone(),
          duration_seconds,
          ffmpeg_bin_dir_used: ffmpeg_bin_dir_used.clone(),
          ffprobe_path: ffprobe_path_text.clone(),
          ffprobe_args: Vec::new(),
          ffprobe_runner: "mf".to_string(),
          cwd: cwd_text.clone(),
          timing_ms,
          debug: Some(debug),
        };

        if let Ok(mut guard) = probe_cache().lock() {
          let entry = guard.entry(cache_key).or_insert(CachedProbeResult {
            input_path: result.input_path.clone(),
            has_duration: false,
            has_tracks: false,
            has_subtitles: false,
            duration_seconds: None,
            audio_streams: Vec::new(),
            subtitle_streams: Vec::new(),
            ffmpeg_bin_dir_used: result.ffmpeg_bin_dir_used.clone(),
            ffprobe_path: result.ffprobe_path.clone(),
            ffprobe_args: result.ffprobe_args.clone(),
            ffprobe_runner: result.ffprobe_runner.clone(),
            cwd: result.cwd.clone(),
          });

          entry.input_path = result.input_path.clone();
          entry.has_duration = true;
          entry.duration_seconds = result.duration_seconds;
          entry.ffmpeg_bin_dir_used = result.ffmpeg_bin_dir_used.clone();
          entry.ffprobe_path = result.ffprobe_path.clone();
          entry.ffprobe_args = result.ffprobe_args.clone();
          entry.ffprobe_runner = result.ffprobe_runner.clone();
          entry.cwd = result.cwd.clone();
        }

        return Ok(result);
      }
    }
  }

  let start_spawn_total = Instant::now();
  let mut cmd = Command::new(&ffprobe_path);
  apply_no_window(&mut cmd);
  cmd.args(&ffprobe_args);
  if let Some(dir) = &workdir {
    cmd.current_dir(dir);
  }
  cmd.stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

  let start_spawn = Instant::now();
  let mut child = cmd.spawn().map_err(|e| {
    if e.kind() == ErrorKind::NotFound {
      "Failed to run ffprobe: program not found (set FFmpeg bin folder or add ffprobe to PATH)".to_string()
    } else {
      format!("Failed to run ffprobe: {e}")
    }
  })?;
  let ffprobe_spawn_ms = start_spawn.elapsed().as_secs_f64() * 1000.0;

  let mut stdout = child
    .stdout
    .take()
    .ok_or_else(|| "Failed to capture ffprobe stdout".to_string())?;
  let mut stderr = child
    .stderr
    .take()
    .ok_or_else(|| "Failed to capture ffprobe stderr".to_string())?;

  let (stdout_tx, stdout_rx) =
    std::sync::mpsc::channel::<(Option<f64>, Vec<u8>, Result<(), String>)>();
  let (stderr_tx, stderr_rx) =
    std::sync::mpsc::channel::<(Option<f64>, Vec<u8>, Result<(), String>)>();

  std::thread::spawn(move || {
    let mut buf = Vec::new();
    let mut first_ms: Option<f64> = None;
    let mut tmp = [0_u8; 8192];
    loop {
      match stdout.read(&mut tmp) {
        Ok(0) => break,
        Ok(n) => {
          if first_ms.is_none() {
            first_ms = Some(start_spawn_total.elapsed().as_secs_f64() * 1000.0);
          }
          buf.extend_from_slice(&tmp[..n]);
        }
        Err(e) => {
          let _ = stdout_tx.send((first_ms, buf, Err(format!("Failed reading ffprobe stdout: {e}"))));
          return;
        }
      }
    }
    let _ = stdout_tx.send((first_ms, buf, Ok(())));
  });

  std::thread::spawn(move || {
    let mut buf = Vec::new();
    let mut first_ms: Option<f64> = None;
    let mut tmp = [0_u8; 8192];
    loop {
      match stderr.read(&mut tmp) {
        Ok(0) => break,
        Ok(n) => {
          if first_ms.is_none() {
            first_ms = Some(start_spawn_total.elapsed().as_secs_f64() * 1000.0);
          }
          buf.extend_from_slice(&tmp[..n]);
        }
        Err(e) => {
          let _ = stderr_tx.send((first_ms, buf, Err(format!("Failed reading ffprobe stderr: {e}"))));
          return;
        }
      }
    }
    let _ = stderr_tx.send((first_ms, buf, Ok(())));
  });

  let start_wait = Instant::now();
  let status = child
    .wait()
    .map_err(|e| format!("Failed waiting for ffprobe: {e}"))?;
  let ffprobe_wait_ms = start_wait.elapsed().as_secs_f64() * 1000.0;

  let (ffprobe_first_stdout_byte_ms, stdout_buf, stdout_ok) =
    stdout_rx.recv().unwrap_or((None, Vec::new(), Err("Failed to receive ffprobe stdout".to_string())));
  let (ffprobe_first_stderr_byte_ms, stderr_buf, stderr_ok) =
    stderr_rx.recv().unwrap_or((None, Vec::new(), Err("Failed to receive ffprobe stderr".to_string())));

  stdout_ok?;
  stderr_ok?;

  if !status.success() {
    let stderr = String::from_utf8_lossy(&stderr_buf).trim().to_string();
    return Err(if stderr.is_empty() {
      "ffprobe failed".to_string()
    } else {
      format!("ffprobe failed: {stderr}")
    });
  }

  let duration_seconds = parse_duration_from_ffprobe_text(&stdout_buf);

  let timing_ms = ProbeTimingInfo {
    validation_ms,
    resolve_binaries_ms,
    ffprobe_spawn_ms,
    ffprobe_first_stdout_byte_ms,
    ffprobe_first_stderr_byte_ms,
    ffprobe_execution_ms: start_spawn_total.elapsed().as_secs_f64() * 1000.0,
    ffprobe_wait_ms,
    json_parsing_ms: 0.0,
    total_ms: start_total.elapsed().as_secs_f64() * 1000.0,
    cache_hit: false,
  };

  let debug = SpawnDebugInfo {
    phase: "duration".to_string(),
    program: ffprobe_path_text.clone(),
    args: ffprobe_args.clone(),
    cwd: cwd_text.clone(),
    program_exists: Path::new(&ffprobe_path_text).exists(),
    exit_code: status.code(),
    success: status.success(),
    stdout_len: stdout_buf.len(),
    stderr_len: stderr_buf.len(),
    stderr_head: stderr_head_text(&stderr_buf),
  };

  let result = DurationProbeResult {
    input_path,
    duration_seconds,
    ffmpeg_bin_dir_used,
    ffprobe_path: ffprobe_path_text,
    ffprobe_args,
    ffprobe_runner: "direct".to_string(),
    cwd: cwd_text,
    timing_ms,
    debug: Some(debug),
  };

  if let Ok(mut guard) = probe_cache().lock() {
    let entry = guard.entry(cache_key).or_insert(CachedProbeResult {
      input_path: result.input_path.clone(),
      has_duration: false,
      has_tracks: false,
      has_subtitles: false,
      duration_seconds: None,
      audio_streams: Vec::new(),
      subtitle_streams: Vec::new(),
      ffmpeg_bin_dir_used: result.ffmpeg_bin_dir_used.clone(),
      ffprobe_path: result.ffprobe_path.clone(),
      ffprobe_args: result.ffprobe_args.clone(),
      ffprobe_runner: result.ffprobe_runner.clone(),
      cwd: result.cwd.clone(),
    });

    entry.input_path = result.input_path.clone();
    entry.has_duration = result.duration_seconds.is_some();
    entry.duration_seconds = result.duration_seconds;
    entry.ffmpeg_bin_dir_used = result.ffmpeg_bin_dir_used.clone();
    entry.ffprobe_path = result.ffprobe_path.clone();
    entry.ffprobe_args = result.ffprobe_args.clone();
    entry.ffprobe_runner = result.ffprobe_runner.clone();
    entry.cwd = result.cwd.clone();
  }

  Ok(result)
}

#[tauri::command]
fn probe_tracks(input_path: String, ffmpeg_bin_dir: String) -> Result<TracksProbeResult, String> {
  use std::time::Instant;
  let start_total = Instant::now();

  let input_path = normalize_input_path_for_cli(&input_path);

  let cache_key = probe_cache_key_best_effort(&input_path);
  if let Ok(guard) = probe_cache().lock() {
    if let Some(cached) = guard.get(&cache_key).cloned() {
      if cached.has_tracks {
        let timing_ms = TracksProbeTimingInfo {
          validation_ms: 0.0,
          resolve_binaries_ms: 0.0,
          audio_ffprobe_ms: 0.0,
          subs_ffprobe_ms: 0.0,
          total_ms: start_total.elapsed().as_secs_f64() * 1000.0,
          cache_hit: true,
        };
        return Ok(TracksProbeResult {
          input_path: cached.input_path,
          audio_streams: cached.audio_streams,
          subtitle_streams: cached.subtitle_streams,
          ffmpeg_bin_dir_used: cached.ffmpeg_bin_dir_used,
          ffprobe_path: cached.ffprobe_path,
          ffprobe_runner: cached.ffprobe_runner,
          cwd: cached.cwd,
          timing_ms,
          debug: Vec::new(),
        });
      }
    }
  }

  let start_validation = Instant::now();
  ensure_input_file_exists(&input_path)?;
  validate_ffmpeg_bin_dir(&ffmpeg_bin_dir)?;
  let validation_ms = start_validation.elapsed().as_secs_f64() * 1000.0;

  let start_resolve = Instant::now();
  let (_ffmpeg_path, ffprobe_path, ffmpeg_bin_dir_used) =
    resolve_ffmpeg_binaries_with_fallback(&ffmpeg_bin_dir);
  let resolve_binaries_ms = start_resolve.elapsed().as_secs_f64() * 1000.0;

  let ffprobe_path_text = ffprobe_path.to_string_lossy().to_string();
  let workdir = stable_working_dir();
  let cwd_text = workdir
    .as_ref()
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_else(|| String::new());

  #[cfg(windows)]
  {
    let start_mf = Instant::now();
    if let Ok(audio_streams) = win_mf::probe_audio_streams(&input_path) {
      let subtitle_streams = if let Ok(guard) = probe_cache().lock() {
        guard
          .get(&cache_key)
          .filter(|c| c.has_subtitles)
          .map(|c| c.subtitle_streams.clone())
          .unwrap_or_default()
      } else {
        Vec::new()
      };

      let timing_ms = TracksProbeTimingInfo {
        validation_ms,
        resolve_binaries_ms,
        audio_ffprobe_ms: start_mf.elapsed().as_secs_f64() * 1000.0,
        subs_ffprobe_ms: 0.0,
        total_ms: start_total.elapsed().as_secs_f64() * 1000.0,
        cache_hit: false,
      };

      let debug = SpawnDebugInfo {
        phase: "tracks_audio_mf".to_string(),
        program: "MediaFoundation".to_string(),
        args: Vec::new(),
        cwd: cwd_text.clone(),
        program_exists: true,
        exit_code: Some(0),
        success: true,
        stdout_len: 0,
        stderr_len: 0,
        stderr_head: String::new(),
      };

      if let Ok(mut guard) = probe_cache().lock() {
        let entry = guard.entry(cache_key).or_insert(CachedProbeResult {
          input_path: input_path.clone(),
          has_duration: false,
          has_tracks: false,
          has_subtitles: false,
          duration_seconds: None,
          audio_streams: Vec::new(),
          subtitle_streams: Vec::new(),
          ffmpeg_bin_dir_used: ffmpeg_bin_dir_used.clone(),
          ffprobe_path: ffprobe_path_text.clone(),
          ffprobe_args: Vec::new(),
          ffprobe_runner: "mf".to_string(),
          cwd: cwd_text.clone(),
        });

        entry.input_path = input_path.clone();
        entry.audio_streams = audio_streams.clone();
        entry.has_tracks = true;
        entry.ffmpeg_bin_dir_used = ffmpeg_bin_dir_used.clone();
        entry.ffprobe_path = ffprobe_path_text.clone();
        entry.ffprobe_runner = "mf".to_string();
        entry.cwd = cwd_text.clone();
      }

      return Ok(TracksProbeResult {
        input_path,
        audio_streams,
        subtitle_streams,
        ffmpeg_bin_dir_used,
        ffprobe_path: ffprobe_path_text,
        ffprobe_runner: "mf".to_string(),
        cwd: cwd_text,
        timing_ms,
        debug: vec![debug],
      });
    }
  }

  let run_json = |phase: &str, args: Vec<String>| -> Result<(Vec<u8>, f64, SpawnDebugInfo), String> {
    let start = Instant::now();
    let mut c = Command::new(&ffprobe_path);
    apply_no_window(&mut c);
    c.args(&args);
    if let Some(dir) = &workdir {
      c.current_dir(dir);
    }
    let out = c
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .output()
      .map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
          "Failed to run ffprobe: program not found (set FFmpeg bin folder or add ffprobe to PATH)".to_string()
        } else {
          format!("Failed to run ffprobe: {e}")
        }
      })?;

    if !out.status.success() {
      let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
      return Err(if stderr.is_empty() {
        "ffprobe failed".to_string()
      } else {
        format!("ffprobe failed: {stderr}")
      });
    }

    let debug = SpawnDebugInfo {
      phase: phase.to_string(),
      program: ffprobe_path_text.clone(),
      args: args.clone(),
      cwd: cwd_text.clone(),
      program_exists: Path::new(&ffprobe_path_text).exists(),
      exit_code: out.status.code(),
      success: out.status.success(),
      stdout_len: out.stdout.len(),
      stderr_len: out.stderr.len(),
      stderr_head: stderr_head_text(&out.stderr),
    };

    Ok((out.stdout, start.elapsed().as_secs_f64() * 1000.0, debug))
  };

  let audio_args: Vec<String> = vec![
    "-v".to_string(),
    "error".to_string(),
    "-print_format".to_string(),
    "json".to_string(),
    "-select_streams".to_string(),
    "a".to_string(),
    "-probesize".to_string(),
    "10M".to_string(),
    "-analyzeduration".to_string(),
    "10M".to_string(),
    "-show_entries".to_string(),
    "stream=index,codec_type,codec_name,channels:stream_tags=language,title".to_string(),
    input_path.clone(),
  ];

  let subs_args: Vec<String> = vec![
    "-v".to_string(),
    "error".to_string(),
    "-print_format".to_string(),
    "json".to_string(),
    "-select_streams".to_string(),
    "s".to_string(),
    "-probesize".to_string(),
    "10M".to_string(),
    "-analyzeduration".to_string(),
    "10M".to_string(),
    "-show_entries".to_string(),
    "stream=index,codec_type,codec_name:stream_tags=language,title".to_string(),
    input_path.clone(),
  ];

  let (audio_stdout, audio_ms, audio_debug) = run_json("tracks_audio", audio_args)?;
  let (mut audio_streams, _subtitle_ignored) = parse_streams_from_ffprobe_json(&audio_stdout)?;

  let (subs_stdout, subs_ms, subs_debug) = run_json("tracks_subs", subs_args)?;
  let (_audio_ignored, subtitle_streams) = parse_streams_from_ffprobe_json(&subs_stdout)?;

  // Ensure audio streams are in stable order by index.
  audio_streams.sort_by(|a, b| a.index.cmp(&b.index));

  let timing_ms = TracksProbeTimingInfo {
    validation_ms,
    resolve_binaries_ms,
    audio_ffprobe_ms: audio_ms,
    subs_ffprobe_ms: subs_ms,
    total_ms: start_total.elapsed().as_secs_f64() * 1000.0,
    cache_hit: false,
  };

  // Update cache with tracks; duration is left as-is.
  if let Ok(mut guard) = probe_cache().lock() {
    let entry = guard.entry(cache_key).or_insert(CachedProbeResult {
      input_path: input_path.clone(),
      has_duration: false,
      has_tracks: false,
      has_subtitles: false,
      duration_seconds: None,
      audio_streams: Vec::new(),
      subtitle_streams: Vec::new(),
      ffmpeg_bin_dir_used: ffmpeg_bin_dir_used.clone(),
      ffprobe_path: ffprobe_path_text.clone(),
      ffprobe_args: Vec::new(),
      ffprobe_runner: "direct".to_string(),
      cwd: cwd_text.clone(),
    });
    entry.audio_streams = audio_streams.clone();
    entry.subtitle_streams = subtitle_streams.clone();
    entry.has_tracks = true;
    entry.has_subtitles = true;
    entry.ffmpeg_bin_dir_used = ffmpeg_bin_dir_used.clone();
    entry.ffprobe_path = ffprobe_path_text.clone();
    entry.ffprobe_runner = "direct".to_string();
    entry.cwd = cwd_text.clone();
  }

  Ok(TracksProbeResult {
    input_path,
    audio_streams,
    subtitle_streams,
    ffmpeg_bin_dir_used,
    ffprobe_path: ffprobe_path_text,
    ffprobe_runner: "direct".to_string(),
    cwd: cwd_text,
    timing_ms,
    debug: vec![audio_debug, subs_debug],
  })
}

#[tauri::command]
fn probe_subtitles(input_path: String, ffmpeg_bin_dir: String) -> Result<SubtitlesProbeResult, String> {
  use std::time::Instant;
  let start_total = Instant::now();

  let input_path = normalize_input_path_for_cli(&input_path);
  let cache_key = probe_cache_key_best_effort(&input_path);

  if let Ok(guard) = probe_cache().lock() {
    if let Some(cached) = guard.get(&cache_key).cloned() {
      if cached.has_subtitles {
        return Ok(SubtitlesProbeResult {
          input_path: cached.input_path,
          subtitle_streams: cached.subtitle_streams,
          ffmpeg_bin_dir_used: cached.ffmpeg_bin_dir_used,
          ffprobe_path: cached.ffprobe_path,
          ffprobe_runner: cached.ffprobe_runner,
          cwd: cached.cwd,
          timing_ms: SubtitlesProbeTimingInfo {
            validation_ms: 0.0,
            resolve_binaries_ms: 0.0,
            ffprobe_ms: 0.0,
            total_ms: start_total.elapsed().as_secs_f64() * 1000.0,
            cache_hit: true,
          },
          debug: None,
        });
      }
    }
  }

  let start_validation = Instant::now();
  ensure_input_file_exists(&input_path)?;
  validate_ffmpeg_bin_dir(&ffmpeg_bin_dir)?;
  let validation_ms = start_validation.elapsed().as_secs_f64() * 1000.0;

  let start_resolve = Instant::now();
  let (_ffmpeg_path, ffprobe_path, ffmpeg_bin_dir_used) =
    resolve_ffmpeg_binaries_with_fallback(&ffmpeg_bin_dir);
  let resolve_binaries_ms = start_resolve.elapsed().as_secs_f64() * 1000.0;

  let ffprobe_path_text = ffprobe_path.to_string_lossy().to_string();
  let workdir = stable_working_dir();
  let cwd_text = workdir
    .as_ref()
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_else(|| String::new());

  let args: Vec<String> = vec![
    "-v".to_string(),
    "error".to_string(),
    "-print_format".to_string(),
    "json".to_string(),
    "-select_streams".to_string(),
    "s".to_string(),
    "-probesize".to_string(),
    "10M".to_string(),
    "-analyzeduration".to_string(),
    "10M".to_string(),
    "-show_entries".to_string(),
    "stream=index,codec_type,codec_name:stream_tags=language,title".to_string(),
    input_path.clone(),
  ];

  let start_ffprobe = Instant::now();
  let mut c = Command::new(&ffprobe_path);
  apply_no_window(&mut c);
  c.args(&args);
  if let Some(dir) = &workdir {
    c.current_dir(dir);
  }
  let out = c
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()
    .map_err(|e| {
      if e.kind() == ErrorKind::NotFound {
        "Failed to run ffprobe: program not found (set FFmpeg bin folder or add ffprobe to PATH)".to_string()
      } else {
        format!("Failed to run ffprobe: {e}")
      }
    })?;

  let ffprobe_ms = start_ffprobe.elapsed().as_secs_f64() * 1000.0;

  let debug = SpawnDebugInfo {
    phase: "subs".to_string(),
    program: ffprobe_path_text.clone(),
    args: args.clone(),
    cwd: cwd_text.clone(),
    program_exists: Path::new(&ffprobe_path_text).exists(),
    exit_code: out.status.code(),
    success: out.status.success(),
    stdout_len: out.stdout.len(),
    stderr_len: out.stderr.len(),
    stderr_head: stderr_head_text(&out.stderr),
  };

  if !out.status.success() {
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    return Err(if stderr.is_empty() {
      "ffprobe failed".to_string()
    } else {
      format!("ffprobe failed: {stderr}")
    });
  }

  let (_audio_ignored, subtitle_streams) = parse_streams_from_ffprobe_json(&out.stdout)?;

  if let Ok(mut guard) = probe_cache().lock() {
    let entry = guard.entry(cache_key).or_insert(CachedProbeResult {
      input_path: input_path.clone(),
      has_duration: false,
      has_tracks: false,
      has_subtitles: false,
      duration_seconds: None,
      audio_streams: Vec::new(),
      subtitle_streams: Vec::new(),
      ffmpeg_bin_dir_used: ffmpeg_bin_dir_used.clone(),
      ffprobe_path: ffprobe_path_text.clone(),
      ffprobe_args: Vec::new(),
      ffprobe_runner: "direct".to_string(),
      cwd: cwd_text.clone(),
    });
    entry.input_path = input_path.clone();
    entry.subtitle_streams = subtitle_streams.clone();
    entry.has_subtitles = true;
    entry.ffmpeg_bin_dir_used = ffmpeg_bin_dir_used.clone();
    entry.ffprobe_path = ffprobe_path_text.clone();
    entry.ffprobe_runner = "direct".to_string();
    entry.cwd = cwd_text.clone();
  }

  Ok(SubtitlesProbeResult {
    input_path,
    subtitle_streams,
    ffmpeg_bin_dir_used,
    ffprobe_path: ffprobe_path_text,
    ffprobe_runner: "direct".to_string(),
    cwd: cwd_text,
    timing_ms: SubtitlesProbeTimingInfo {
      validation_ms,
      resolve_binaries_ms,
      ffprobe_ms,
      total_ms: start_total.elapsed().as_secs_f64() * 1000.0,
      cache_hit: false,
    },
    debug: Some(debug),
  })
}

fn build_output_path(input_path: &str, mode: &str, in_time: &str, out_time: &str) -> Result<PathBuf, String> {
  let input = Path::new(input_path);
  let parent = input
    .parent()
    .ok_or_else(|| "Could not determine input folder".to_string())?;
  let stem = input
    .file_stem()
    .ok_or_else(|| "Could not determine input filename".to_string())?
    .to_string_lossy();
  let extension = input
    .extension()
    .map(|e| e.to_string_lossy().to_string())
    .unwrap_or_else(|| "mp4".to_string());

  let suffix_in = time_for_filename(in_time);
  let suffix_out = time_for_filename(out_time);
  let filename = format!(
    "{}_clip_{}_{}_{}.{}",
    stem,
    mode,
    suffix_in,
    suffix_out,
    extension
  );
  Ok(parent.join(filename))
}

#[tauri::command]
fn detect_ffmpeg_bin_dir(ffmpeg_bin_dir: String) -> Result<String, String> {
  validate_ffmpeg_bin_dir(&ffmpeg_bin_dir)?;
  let (_ffmpeg, _ffprobe, used) = resolve_ffmpeg_binaries_with_fallback(&ffmpeg_bin_dir);
  Ok(used)
}

fn find_winget_path() -> Option<PathBuf> {
  if let Some(p) = winget_windowsapps_stub_path() {
    return Some(p);
  }

  let mut cmd = Command::new("where.exe");
  apply_no_window(&mut cmd);
  let output = cmd
    .arg("winget")
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .output();

  if let Ok(output) = output {
    if output.status.success() {
      let stdout = String::from_utf8_lossy(&output.stdout);
      if let Some(first) = stdout.lines().next().map(str::trim).filter(|l| !l.is_empty()) {
        return Some(PathBuf::from(first));
      }
    }
  }

  None
}

#[tauri::command]
fn check_winget() -> Result<WingetStatusResult, String> {
  if !cfg!(windows) {
    return Ok(WingetStatusResult {
      available: false,
      message: "WinGet is Windows-only.".to_string(),
    });
  }

  let Some(winget) = find_winget_path() else {
    return Ok(WingetStatusResult {
      available: false,
      message:
        "WinGet not found. If App Installer is installed, enable the WinGet App Execution Alias (Settings → Apps → Advanced app settings → App execution aliases).".to_string(),
    });
  };

  let mut cmd = Command::new(winget);
  apply_no_window(&mut cmd);
  let output = cmd
    .arg("--version")
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output();

  match output {
    Ok(o) if o.status.success() => Ok(WingetStatusResult {
      available: true,
      message: "WinGet detected.".to_string(),
    }),
    Ok(o) => {
      let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
      Ok(WingetStatusResult {
        available: false,
        message: if stderr.is_empty() {
          "WinGet failed to run.".to_string()
        } else {
          format!("WinGet failed to run: {stderr}")
        },
      })
    }
    Err(e) => Ok(WingetStatusResult {
      available: false,
      message: format!("Failed to run WinGet: {e}"),
    }),
  }
}

#[tauri::command]
fn check_ffmpeg(ffmpeg_bin_dir: String) -> Result<FfmpegCheckResult, String> {
  validate_ffmpeg_bin_dir(&ffmpeg_bin_dir)?;

  let (ffmpeg_path, ffprobe_path, ffmpeg_bin_dir_used) =
    resolve_ffmpeg_binaries_with_fallback(&ffmpeg_bin_dir);

  let run = |exe: &Path, name: &str| -> Result<(), String> {
    let mut cmd = Command::new(exe);
    apply_no_window(&mut cmd);
    let output = cmd
      .arg("-version")
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .output()
      .map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
          format!("{name} not found")
        } else {
          format!("Failed to run {name}: {e}")
        }
      })?;

    if !output.status.success() {
      let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
      return Err(if stderr.is_empty() {
        format!("{name} failed")
      } else {
        format!("{name} failed: {stderr}")
      });
    }

    Ok(())
  };

  let ffmpeg_ok = run(&ffmpeg_path, "ffmpeg");
  let ffprobe_ok = run(&ffprobe_path, "ffprobe");

  if ffmpeg_ok.is_ok() && ffprobe_ok.is_ok() {
    return Ok(FfmpegCheckResult {
      ok: true,
      message: "FFmpeg detected.".to_string(),
      ffmpeg_bin_dir_used,
    });
  }

  let mut details = Vec::new();
  if let Err(e) = ffmpeg_ok {
    details.push(e);
  }
  if let Err(e) = ffprobe_ok {
    details.push(e);
  }

  Ok(FfmpegCheckResult {
    ok: false,
    message: if details.is_empty() {
      "FFmpeg not found.".to_string()
    } else {
      details.join(" | ")
    },
    ffmpeg_bin_dir_used,
  })
}

#[tauri::command]
fn install_ffmpeg_winget() -> Result<(), String> {
  if !cfg!(windows) {
    return Err("WinGet install is only supported on Windows.".to_string());
  }

  let winget_path = find_winget_path().ok_or_else(|| {
    "WinGet not found. If App Installer is installed, enable the WinGet App Execution Alias (Settings -> Apps -> Advanced app settings -> App execution aliases).".to_string()
  })?;
  let winget_str = winget_path.to_string_lossy().replace('\'', "''");

  let cmd = format!(
    "& '{}' install -e --id Gyan.FFmpeg --accept-source-agreements --accept-package-agreements",
    winget_str
  );

  let mut ps = Command::new("powershell.exe");
  apply_no_window(&mut ps);
  ps.args(["-ExecutionPolicy", "Bypass", "-Command", &cmd])
    .spawn()
    .map_err(|e| {
      if e.kind() == ErrorKind::NotFound {
        "Failed to open PowerShell.".to_string()
      } else {
        format!("Failed to start WinGet install: {e}")
      }
    })?;

  Ok(())
}

/// Probe the duration of a media file using ffprobe (returns seconds).
fn probe_duration_ffprobe(ffprobe_path: &Path, file_path: &Path) -> Option<f64> {
  let mut cmd = Command::new(ffprobe_path);
  apply_no_window(&mut cmd);
  let output = cmd
    .args([
      "-v", "error",
      "-show_entries", "format=duration",
      "-of", "default=noprint_wrappers=1:nokey=1",
    ])
    .arg(file_path)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()
    .ok()?;
  let stdout = String::from_utf8_lossy(&output.stdout);
  stdout.trim().parse::<f64>().ok()
}

fn run_ffprobe_keyframes(
  ffprobe_path: &Path,
  input_path: &str,
  read_intervals: &str,
) -> Result<Vec<f64>, String> {
  let mut cmd = Command::new(ffprobe_path);
  apply_no_window(&mut cmd);
  let output = cmd
    .args([
      "-v",
      "quiet",
      "-select_streams",
      "v:0",
      "-skip_frame",
      "nokey",
      "-read_intervals",
      read_intervals,
      "-print_format",
      "json",
      "-show_frames",
      "-show_entries",
      "frame=best_effort_timestamp_time",
    ])
    .arg(input_path)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()
    .map_err(|e| {
      if e.kind() == ErrorKind::NotFound {
        "Failed to run ffprobe: program not found (set FFmpeg bin folder or add ffprobe to PATH)".to_string()
      } else {
        format!("Failed to run ffprobe: {e}")
      }
    })?;

  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    return Err(if stderr.is_empty() {
      "ffprobe failed".to_string()
    } else {
      format!("ffprobe failed: {stderr}")
    });
  }

  let json: serde_json::Value =
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid ffprobe JSON: {e}"))?;

  let mut times = Vec::new();
  if let Some(frames) = json.get("frames").and_then(|f| f.as_array()) {
    for frame in frames {
      if let Some(ts) = frame.get("best_effort_timestamp_time").and_then(|v| v.as_str()) {
        if let Ok(v) = ts.parse::<f64>() {
          times.push(v);
        }
      }
    }
  }
  Ok(times)
}

#[tauri::command]
async fn lossless_preflight(input_path: String, in_time: String, out_time: String, ffmpeg_bin_dir: String) -> Result<LosslessPreflightResult, String> {
  tauri::async_runtime::spawn_blocking(move || lossless_preflight_sync(input_path, in_time, out_time, ffmpeg_bin_dir))
    .await
    .map_err(|e| format!("lossless_preflight failed: {e}"))?
}

/// Find the last keyframe at or before `target` and the first keyframe at or after `target`.
fn find_surrounding_keyframes(ffprobe_path: &Path, input_path: &str, target: f64) -> (Option<f64>, Option<f64>) {
  let windows = [60.0_f64, 600.0_f64, 3600.0_f64];

  // Keyframe before (or at) target
  let mut prev: Option<f64> = None;
  for w in windows {
    let start = (target - w).max(0.0);
    let read_intervals = format!("{start}%{target}");
    if let Ok(mut times) = run_ffprobe_keyframes(ffprobe_path, input_path, &read_intervals) {
      times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
      if let Some(last) = times.last().copied() {
        prev = Some(last);
        break;
      }
    }
  }

  // Keyframe after (or at) target
  let mut next: Option<f64> = None;
  for w in windows {
    let end = target + w;
    let read_intervals = format!("{target}%{end}");
    if let Ok(mut times) = run_ffprobe_keyframes(ffprobe_path, input_path, &read_intervals) {
      times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
      if let Some(first) = times.into_iter().find(|t| *t + 1e-6 >= target) {
        next = Some(first);
        break;
      }
    }
  }

  // Round to millisecond precision
  let prev = prev.map(|v| (v * 1000.0).round() / 1000.0);
  let next = next.map(|v| (v * 1000.0).round() / 1000.0);
  (prev, next)
}

fn lossless_preflight_sync(input_path: String, in_time: String, out_time: String, ffmpeg_bin_dir: String) -> Result<LosslessPreflightResult, String> {
  ensure_input_file_exists(&input_path)?;
  validate_ffmpeg_bin_dir(&ffmpeg_bin_dir)?;

  let in_seconds = parse_hh_mm_ss_with_millis(&in_time)?;
  let out_seconds = parse_hh_mm_ss_with_millis(&out_time)?;

  let (_ffmpeg_path, ffprobe_path, _ffmpeg_bin_dir_used) =
    resolve_ffmpeg_binaries_with_fallback(&ffmpeg_bin_dir);

  // --- IN point analysis ---
  let (nearest, next) = if in_seconds <= 0.0 {
    (Some(0.0), Some(0.0))
  } else {
    find_surrounding_keyframes(&ffprobe_path, &input_path, in_seconds)
  };

  let start_shift_seconds = nearest.map(|kf| {
    if kf <= in_seconds { (in_seconds - kf).max(0.0) } else { 0.0 }
  });

  // --- OUT point analysis ---
  let (out_prev, out_next) = find_surrounding_keyframes(&ffprobe_path, &input_path, out_seconds);

  let end_shift_seconds = out_next.map(|kf| {
    if kf > out_seconds + 1e-6 { (kf - out_seconds).max(0.0) } else { 0.0 }
  });

  Ok(LosslessPreflightResult {
    in_time_seconds: in_seconds,
    nearest_keyframe_seconds: nearest,
    next_keyframe_seconds: next,
    start_shift_seconds,
    out_time_seconds: Some(out_seconds),
    out_prev_keyframe_seconds: out_prev,
    out_next_keyframe_seconds: out_next,
    end_shift_seconds,
  })
}

#[tauri::command]
fn probe_media(input_path: String, ffmpeg_bin_dir: String) -> Result<ProbeResult, String> {
  use std::time::Instant;
  let start_total = Instant::now();

  let input_path = normalize_input_path_for_cli(&input_path);

  let start_validation = Instant::now();
  ensure_input_file_exists(&input_path)?;
  validate_ffmpeg_bin_dir(&ffmpeg_bin_dir)?;
  let validation_ms = start_validation.elapsed().as_secs_f64() * 1000.0;
  eprintln!("[PERF] Validation took: {:?}", start_validation.elapsed());

  let start_resolve = Instant::now();
  let (_ffmpeg_path, ffprobe_path, ffmpeg_bin_dir_used) =
    resolve_ffmpeg_binaries_with_fallback(&ffmpeg_bin_dir);
  let resolve_binaries_ms = start_resolve.elapsed().as_secs_f64() * 1000.0;
  eprintln!("[PERF] Resolve binaries took: {:?}", start_resolve.elapsed());

  let ffprobe_path_text = ffprobe_path.to_string_lossy().to_string();
  let workdir = stable_working_dir();
  let cwd_text = workdir
    .as_ref()
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_else(|| String::new());

  let cache_key = probe_cache_key_best_effort(&input_path);
  if let Ok(guard) = probe_cache().lock() {
    if let Some(cached) = guard.get(&cache_key).cloned() {
      let timing_ms = ProbeTimingInfo {
        validation_ms,
        resolve_binaries_ms,
        ffprobe_spawn_ms: 0.0,
        ffprobe_first_stdout_byte_ms: None,
        ffprobe_first_stderr_byte_ms: None,
        ffprobe_execution_ms: 0.0,
        ffprobe_wait_ms: 0.0,
        json_parsing_ms: 0.0,
        total_ms: start_total.elapsed().as_secs_f64() * 1000.0,
        cache_hit: true,
      };

      return Ok(ProbeResult {
        input_path: cached.input_path,
        duration_seconds: cached.duration_seconds,
        audio_streams: cached.audio_streams,
        subtitle_streams: cached.subtitle_streams,
        ffmpeg_bin_dir_used: cached.ffmpeg_bin_dir_used,
        ffprobe_path: cached.ffprobe_path,
        ffprobe_args: cached.ffprobe_args,
        ffprobe_runner: cached.ffprobe_runner,
        cwd: cached.cwd,
        timing_ms,
      });
    }
  }

  let start_spawn_total = Instant::now();
  let ffprobe_args: Vec<String> = vec![
    // Keep this probe intentionally lightweight: only request the fields we actually use.
    // Full `-show_streams -show_format` can be much slower on some systems/files.
    "-v".to_string(),
    "error".to_string(),
    "-print_format".to_string(),
    "json".to_string(),
    "-show_entries".to_string(),
    "format=duration:stream=index,codec_type,codec_name,channels:stream_tags=language,title".to_string(),
    input_path.clone(),
  ];

  let mut cmd = Command::new(&ffprobe_path);
  apply_no_window(&mut cmd);
  cmd.args(&ffprobe_args);
  if let Some(dir) = &workdir {
    cmd.current_dir(dir);
  }
  let ffprobe_runner = "direct".to_string();

  cmd.stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

  let start_spawn = Instant::now();
  let mut child = cmd.spawn().map_err(|e| {
    if e.kind() == ErrorKind::NotFound {
      "Failed to run ffprobe: program not found (set FFmpeg bin folder or add ffprobe to PATH)".to_string()
    } else {
      format!("Failed to run ffprobe: {e}")
    }
  })?;
  let ffprobe_spawn_ms = start_spawn.elapsed().as_secs_f64() * 1000.0;

  let mut stdout = child
    .stdout
    .take()
    .ok_or_else(|| "Failed to capture ffprobe stdout".to_string())?;
  let mut stderr = child
    .stderr
    .take()
    .ok_or_else(|| "Failed to capture ffprobe stderr".to_string())?;

  let (stdout_tx, stdout_rx) =
    std::sync::mpsc::channel::<(Option<f64>, Vec<u8>, Result<(), String>)>();
  let (stderr_tx, stderr_rx) =
    std::sync::mpsc::channel::<(Option<f64>, Vec<u8>, Result<(), String>)>();

  std::thread::spawn(move || {
    let mut buf = Vec::new();
    let mut first_ms: Option<f64> = None;
    let mut tmp = [0_u8; 8192];
    loop {
      match stdout.read(&mut tmp) {
        Ok(0) => break,
        Ok(n) => {
          if first_ms.is_none() {
            first_ms = Some(start_spawn_total.elapsed().as_secs_f64() * 1000.0);
          }
          buf.extend_from_slice(&tmp[..n]);
        }
        Err(e) => {
          let _ = stdout_tx.send((first_ms, buf, Err(format!("Failed reading ffprobe stdout: {e}"))));
          return;
        }
      }
    }
    let _ = stdout_tx.send((first_ms, buf, Ok(())));
  });

  std::thread::spawn(move || {
    let mut buf = Vec::new();
    let mut first_ms: Option<f64> = None;
    let mut tmp = [0_u8; 8192];
    loop {
      match stderr.read(&mut tmp) {
        Ok(0) => break,
        Ok(n) => {
          if first_ms.is_none() {
            first_ms = Some(start_spawn_total.elapsed().as_secs_f64() * 1000.0);
          }
          buf.extend_from_slice(&tmp[..n]);
        }
        Err(e) => {
          let _ = stderr_tx.send((first_ms, buf, Err(format!("Failed reading ffprobe stderr: {e}"))));
          return;
        }
      }
    }
    let _ = stderr_tx.send((first_ms, buf, Ok(())));
  });

  let start_wait = Instant::now();
  let status = child
    .wait()
    .map_err(|e| format!("Failed waiting for ffprobe: {e}"))?;
  let ffprobe_wait_ms = start_wait.elapsed().as_secs_f64() * 1000.0;

  let (ffprobe_first_stdout_byte_ms, stdout_buf, stdout_ok) =
    stdout_rx.recv().unwrap_or((None, Vec::new(), Err("Failed to receive ffprobe stdout".to_string())));
  let (ffprobe_first_stderr_byte_ms, stderr_buf, stderr_ok) =
    stderr_rx.recv().unwrap_or((None, Vec::new(), Err("Failed to receive ffprobe stderr".to_string())));

  stdout_ok?;
  stderr_ok?;

  eprintln!("[PERF] FFprobe execution took: {:?}", start_spawn_total.elapsed());

  if !status.success() {
    let stderr = String::from_utf8_lossy(&stderr_buf).trim().to_string();
    return Err(if stderr.is_empty() {
      "ffprobe failed".to_string()
    } else {
      format!("ffprobe failed: {stderr}")
    });
  }

  let start_parse = Instant::now();
  let json: serde_json::Value =
    serde_json::from_slice(&stdout_buf).map_err(|e| format!("Invalid ffprobe JSON: {e}"))?;
  eprintln!("[PERF] JSON parsing took: {:?}", start_parse.elapsed());

  let duration_seconds = json
    .get("format")
    .and_then(|f| f.get("duration"))
    .and_then(|d| d.as_str())
    .and_then(|s| s.parse::<f64>().ok());

  let mut audio_streams = Vec::new();
  let mut subtitle_streams = Vec::new();
  if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
    for stream in streams {
      let codec_type = stream.get("codec_type").and_then(|t| t.as_str()).unwrap_or("");
      if codec_type != "audio" && codec_type != "subtitle" {
        continue;
      }

      let index = stream
        .get("index")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "ffprobe stream missing index".to_string())? as i32;
      let codec_name = stream
        .get("codec_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
      let (language, title) = stream
        .get("tags")
        .and_then(|t| t.as_object())
        .map(|tags| {
          let language = tags
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("und")
            .to_string();
          let title = tags
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
          (language, title)
        })
        .unwrap_or_else(|| ("und".to_string(), "".to_string()));

      if codec_type == "audio" {
        let channels = stream
          .get("channels")
          .and_then(|v| v.as_i64())
          .map(|v| v as i32);

        audio_streams.push(AudioStreamInfo {
          order: 0,
          index,
          codec_name,
          channels,
          language,
          title,
        });
      } else {
        subtitle_streams.push(SubtitleStreamInfo {
          order: 0,
          index,
          codec_name,
          language,
          title,
        });
      }
    }
  }

  audio_streams.sort_by(|a, b| a.index.cmp(&b.index));
  for (i, s) in audio_streams.iter_mut().enumerate() {
    s.order = i as i32;
  }
  subtitle_streams.sort_by(|a, b| a.index.cmp(&b.index));
  for (i, s) in subtitle_streams.iter_mut().enumerate() {
    s.order = i as i32;
  }

  let timing_ms = ProbeTimingInfo {
    validation_ms,
    resolve_binaries_ms,
    ffprobe_spawn_ms,
    ffprobe_first_stdout_byte_ms,
    ffprobe_first_stderr_byte_ms,
    ffprobe_execution_ms: start_spawn_total.elapsed().as_secs_f64() * 1000.0,
    ffprobe_wait_ms,
    json_parsing_ms: start_parse.elapsed().as_secs_f64() * 1000.0,
    total_ms: start_total.elapsed().as_secs_f64() * 1000.0,
    cache_hit: false,
  };

  eprintln!("[PERF] TOTAL probe_media took: {:?}", start_total.elapsed());

  let result = ProbeResult {
    input_path: input_path.clone(),
    duration_seconds,
    audio_streams,
    subtitle_streams,
    ffmpeg_bin_dir_used,
    ffprobe_path: ffprobe_path_text,
    ffprobe_args: ffprobe_args.clone(),
    ffprobe_runner,
    cwd: cwd_text,
    timing_ms,
  };

  if let Ok(mut guard) = probe_cache().lock() {
    guard.insert(
      cache_key,
      CachedProbeResult {
        input_path: result.input_path.clone(),
        has_duration: result.duration_seconds.is_some(),
        has_tracks: !result.audio_streams.is_empty() || !result.subtitle_streams.is_empty(),
        has_subtitles: true,
        duration_seconds: result.duration_seconds,
        audio_streams: result.audio_streams.clone(),
        subtitle_streams: result.subtitle_streams.clone(),
        ffmpeg_bin_dir_used: result.ffmpeg_bin_dir_used.clone(),
        ffprobe_path: result.ffprobe_path.clone(),
        ffprobe_args: result.ffprobe_args.clone(),
        ffprobe_runner: result.ffprobe_runner.clone(),
        cwd: result.cwd.clone(),
      },
    );
  }

  Ok(result)
}

#[tauri::command]
fn trim_media(
  window: tauri::Window,
  input_path: String,
  in_time: String,
  out_time: String,
  mode: String,
  audio_stream_index: i32,
  subtitle_stream_index: i32,
  ffmpeg_bin_dir: String,
) -> Result<TrimResult, String> {
  ensure_input_file_exists(&input_path)?;
  validate_ffmpeg_bin_dir(&ffmpeg_bin_dir)?;

  // Parse with millisecond precision to preserve exact keyframe times
  let in_seconds_f64 = parse_hh_mm_ss_with_millis(&in_time)?;
  let out_seconds_f64 = parse_hh_mm_ss_with_millis(&out_time)?;
  if out_seconds_f64 <= in_seconds_f64 {
    return Err("OUT must be greater than IN".to_string());
  }

  // For file existence check and old code compatibility, also get whole seconds
  let _in_seconds = in_seconds_f64.floor() as u64;
  let _out_seconds = out_seconds_f64.floor() as u64;

  let mode = mode.trim().to_lowercase();
  if mode != "lossless" && mode != "exact" {
    return Err("Mode must be 'lossless' or 'exact'".to_string());
  }

  let output_path = {
    let base = build_output_path(&input_path, &mode, &in_time, &out_time)?;
    if !base.exists() {
      base
    } else {
      // Auto-number: file (1), file (2), etc.
      let stem = base.file_stem().unwrap_or_default().to_string_lossy().to_string();
      let ext = base.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
      let parent = base.parent().unwrap_or_else(|| Path::new("."));
      let mut numbered = base.clone();
      for i in 1..=999 {
        numbered = parent.join(format!("{stem} ({i}).{ext}"));
        if !numbered.exists() { break; }
      }
      numbered
    }
  };

  let (ffmpeg_path, ffprobe_path, _ffmpeg_bin_dir_used) =
    resolve_ffmpeg_binaries_with_fallback(&ffmpeg_bin_dir);

  let rotation_degrees = probe_video_rotation_degrees_best_effort(&ffprobe_path, &input_path);
  let rotation_filter = rotation_filter_for_degrees(rotation_degrees);

  if mode == "lossless" && rotation_degrees != 0 {
    return Err(format!(
      "Lossless cannot reliably preserve vertical orientation (input is rotated {rotation_degrees}°). Use Exact mode."
    ));
  }

  let mut cmd = Command::new(ffmpeg_path);
  apply_no_window(&mut cmd);

  // For millisecond precision, pass time as decimal seconds (e.g., "3.170000")
  let in_time_arg = format!("{:.6}", in_seconds_f64);
  let duration = out_seconds_f64 - in_seconds_f64;
  let duration_arg = format!("{:.6}", duration);

  if mode == "lossless" {
    // LOSSLESS: -ss BEFORE -i for input-level seeking, with -t for duration.
    // Placing -ss before -i makes FFmpeg seek to the nearest keyframe <= IN
    // at the demuxer level.  With -c copy the output starts from that keyframe
    // (so the start may be slightly early), and -t counts from the actual
    // output start, giving the correct requested duration.
    //
    // Previously -ss was placed AFTER -i, which caused -t to count from the
    // seek point while the output started at the earlier keyframe, inflating
    // the output duration by the keyframe-to-IN gap.
    cmd.args(["-v", "error", "-progress", "pipe:1"])
      .args(["-ss"]).arg(&in_time_arg)
      .args(["-i"]).arg(&input_path)
      .args(["-t"]).arg(&duration_arg);
  } else {
    // EXACT: -ss BEFORE -i for fast seeking, then re-encode for frame accuracy.
    cmd.args(["-v", "error", "-progress", "pipe:1", "-accurate_seek", "-ss"])
      .arg(&in_time_arg);

    if rotation_filter.is_some() {
      cmd.arg("-noautorotate");
    }

    cmd.args(["-i"]).arg(&input_path)
      .args(["-t"]).arg(&duration_arg);
  }

  cmd.args(["-map", "0:v:0"]);

  if audio_stream_index < 0 {
    cmd.arg("-an");
  } else {
    // `audio_stream_index` is treated as the 0-based order within audio streams (not the global ffprobe stream index).
    cmd.args(["-map", &format!("0:a:{audio_stream_index}")]);
  }

  if subtitle_stream_index >= 0 && mode != "lossless" {
    // Subtitles are excluded in lossless mode: subtitle packets can span the
    // cut boundary and force FFmpeg to extend the output duration beyond the
    // requested range.  Exact mode re-encodes everything so it trims cleanly.
    cmd.args(["-map", &format!("0:{subtitle_stream_index}")]);
  }

  if mode == "lossless" {
    // Determine output container from extension
    let output_ext = output_path
      .extension()
      .map(|e| e.to_string_lossy().to_lowercase())
      .unwrap_or_default();

    cmd.args(["-c", "copy"]);

    // MP4 container needs different timestamp handling than MKV
    if output_ext == "mp4" || output_ext == "m4v" || output_ext == "mov" {
      // For MP4: avoid_negative_ts with make_zero and fflags to fix timestamps
      cmd.args(["-avoid_negative_ts", "make_zero", "-fflags", "+genpts"]);
    } else {
      // For MKV and other containers: copyts works better
      cmd.args(["-copyts", "-avoid_negative_ts", "make_zero"]);
    }

    if rotation_degrees != 0 {
      cmd.args(["-metadata:s:v:0", &format!("rotate={rotation_degrees}")]);
    }
  } else {
    if let Some(filter) = rotation_filter {
      cmd.arg("-vf").arg(filter);
      cmd.args(["-metadata:s:v:0", "rotate=0"]);
    }

    cmd.args([
      "-c:v",
      "libx264",
      "-crf",
      "18",
      "-preset",
      "veryfast",
      "-pix_fmt",
      "yuv420p",
    ]);

    if audio_stream_index >= 0 {
      cmd.args(["-c:a", "copy"]);
    }

    if subtitle_stream_index >= 0 {
      cmd.args(["-c:s", "copy"]);
      // Subtitle packet durations can extend past the requested cut end
      // (e.g., a cue that starts before OUT but ends after it). Clamp output
      // to the shortest mapped stream so Exact mode duration stays precise.
      cmd.arg("-shortest");
    }
  }

  cmd.arg("-y")
    .arg(&output_path)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

  let mut child = cmd
    .spawn()
    .map_err(|e| {
      if e.kind() == ErrorKind::NotFound {
        "Failed to run ffmpeg: program not found (set FFmpeg bin folder or add ffmpeg to PATH)".to_string()
      } else {
        format!("Failed to run ffmpeg: {e}")
      }
    })?;

  // Read stdout for `-progress pipe:1` output and emit progress events.
  // FFmpeg writes key=value lines; we parse `out_time_us` for current position.
  let duration_us = (duration * 1_000_000.0) as i64;
  if let Some(stdout) = child.stdout.take() {
    let reader = std::io::BufReader::new(stdout);
    let mut last_pct: i32 = -1;
    use std::io::BufRead;
    for line in reader.lines() {
      let line = match line { Ok(l) => l, Err(_) => break };
      if let Some(val) = line.strip_prefix("out_time_us=") {
        if let Ok(us) = val.trim().parse::<i64>() {
          let pct = if duration_us > 0 {
            ((us as f64 / duration_us as f64) * 100.0).round().min(100.0) as i32
          } else { 0 };
          if pct != last_pct {
            last_pct = pct;
            let _ = window.emit("cut_progress", serde_json::json!({ "percent": pct }));
          }
        }
      }
    }
  }

  let status = child.wait().map_err(|e| format!("Failed to wait for ffmpeg: {e}"))?;
  let stderr_bytes = child.stderr.take().map(|mut s| {
    let mut buf = Vec::new();
    let _ = std::io::Read::read_to_end(&mut s, &mut buf);
    buf
  }).unwrap_or_default();

  if !status.success() {
    let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
    return Err(if stderr.is_empty() {
      "ffmpeg failed".to_string()
    } else {
      format!("ffmpeg failed: {stderr}")
    });
  }

  // Validate output file size - a file under 10KB is likely corrupt/empty
  let output_size = std::fs::metadata(&output_path)
    .map(|m| m.len())
    .unwrap_or(0);
  if output_size < 10_000 {
    // Clean up the corrupt file
    let _ = std::fs::remove_file(&output_path);
    return Err(format!(
      "Lossless cut produced invalid output ({} bytes). This usually happens when the cut point is not near a keyframe. Try using 'Exact' mode instead, or adjust the cut times to be closer to a keyframe.",
      output_size
    ));
  }

  // Post-cut: probe actual output duration and warn if it differs significantly
  let requested_duration = out_seconds_f64 - in_seconds_f64;
  let actual_duration = probe_duration_ffprobe(&ffprobe_path, &output_path);
  let duration_warning = actual_duration.and_then(|actual| {
    let diff = (actual - requested_duration).abs();
    if diff > 0.5 {
      Some(format!(
        "Output duration is {:.1}s (requested {:.1}s, difference {:.1}s). Lossless cuts can only split on keyframes, so the result may be slightly shorter or longer.",
        actual, requested_duration, diff
      ))
    } else {
      None
    }
  });

  Ok(TrimResult {
    output_path: output_path.to_string_lossy().to_string(),
    requested_duration_seconds: requested_duration,
    actual_duration_seconds: actual_duration,
    duration_warning,
  })
}

#[tauri::command]
fn add_defender_exclusion(path: String) -> Result<(), String> {
  if !cfg!(windows) {
    return Err("Windows Defender exclusion is only supported on Windows.".to_string());
  }

  if path.trim().is_empty() {
    return Err("Path cannot be empty.".to_string());
  }

  let path_escaped = path.replace('\'', "''");
  let cmd = format!(
    "Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', \"Add-MpPreference -ExclusionPath '{}'\"",
    path_escaped
  );

  let mut ps = Command::new("powershell.exe");
  apply_no_window(&mut ps);
  ps.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &cmd])
    .spawn()
    .map_err(|e| {
      if e.kind() == ErrorKind::NotFound {
        "Failed to open PowerShell.".to_string()
      } else {
        format!("Failed to add exclusion: {e}")
      }
    })?;

  Ok(())
}

#[derive(Debug, Serialize)]
struct DefenderCheckResult {
  is_windows: bool,
  defender_running: bool,
  ffmpeg_path: String,
}

#[tauri::command]
fn check_defender_exclusion_needed(ffmpeg_bin_dir: String) -> Result<DefenderCheckResult, String> {
  if !cfg!(windows) {
    return Ok(DefenderCheckResult {
      is_windows: false,
      defender_running: false,
      ffmpeg_path: String::new(),
    });
  }

  let (_ffmpeg, _ffprobe, ffmpeg_path) = resolve_ffmpeg_binaries_with_fallback(&ffmpeg_bin_dir);

  // Check if Windows Defender service is running
  let mut sc = Command::new("sc");
  apply_no_window(&mut sc);
  let defender_running = sc
    .args(["query", "WinDefend"])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .output()
    .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("RUNNING"))
    .unwrap_or(false);

  Ok(DefenderCheckResult {
    is_windows: true,
    defender_running,
    ffmpeg_path,
  })
}

#[tauri::command]
fn get_app_dir() -> Result<String, String> {
  env::current_exe()
    .ok()
    .and_then(|exe| exe.parent().map(|p| p.to_string_lossy().to_string()))
    .ok_or_else(|| "Failed to get app directory".to_string())
}

fn get_bundled_ffmpeg_dir() -> Option<PathBuf> {
  // Check %LOCALAPPDATA%\Clip Wave\bin first
  if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
    let bundled_bin = PathBuf::from(local_app_data).join("Clip Wave").join("bin");
    if looks_like_ffmpeg_bin_dir(&bundled_bin) {
      return Some(bundled_bin);
    }
  }

  // Fallback to app directory
  if let Ok(exe) = env::current_exe() {
    if let Some(exe_dir) = exe.parent() {
      let bundled_bin = exe_dir.join("bin");
      if looks_like_ffmpeg_bin_dir(&bundled_bin) {
        return Some(bundled_bin);
      }
    }
  }

  None
}

#[tauri::command]
async fn download_ffmpeg_direct(window: tauri::Window) -> Result<String, String> {
  tauri::async_runtime::spawn_blocking(move || download_ffmpeg_direct_sync(window))
    .await
    .map_err(|e| format!("download_ffmpeg_direct failed: {e}"))?
}

#[derive(Clone, serde::Serialize)]
struct FfmpegInstallProgress {
  phase: String,
  message: String,
  progress: Option<f64>,
  bytes_done: Option<u64>,
  bytes_total: Option<u64>,
}

fn emit_ffmpeg_install_progress(
  window: &tauri::Window,
  phase: &str,
  message: &str,
  progress: Option<f64>,
  bytes_done: Option<u64>,
  bytes_total: Option<u64>,
) {
  let _ = window.emit(
    "ffmpeg_install_progress",
    FfmpegInstallProgress {
      phase: phase.to_string(),
      message: message.to_string(),
      progress,
      bytes_done,
      bytes_total,
    },
  );
}

fn count_files_recursive(dir: &Path) -> std::io::Result<u64> {
  let mut count = 0_u64;
  for entry in fs::read_dir(dir)? {
    let entry = entry?;
    let path = entry.path();
    let file_type = entry.file_type()?;
    if file_type.is_dir() {
      count += count_files_recursive(&path)?;
    } else if file_type.is_file() {
      count += 1;
    }
  }
  Ok(count)
}

fn copy_dir_recursive_with_progress(
  src: &Path,
  dst: &Path,
  mut on_progress: impl FnMut(u64, u64),
) -> std::io::Result<()> {
  let total = count_files_recursive(src).unwrap_or(0);
  let mut done = 0_u64;

  fn walk(
    src: &Path,
    dst: &Path,
    done: &mut u64,
    total: u64,
    on_progress: &mut impl FnMut(u64, u64),
  ) -> std::io::Result<()> {
    if !dst.exists() {
      fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
      let entry = entry?;
      let path = entry.path();
      let file_type = entry.file_type()?;
      let target = dst.join(entry.file_name());
      if file_type.is_dir() {
        walk(&path, &target, done, total, on_progress)?;
      } else if file_type.is_file() {
        let _ = fs::remove_file(&target);
        fs::copy(&path, &target)?;
        *done += 1;
        on_progress(*done, total.max(1));
      }
    }
    Ok(())
  }

  walk(src, dst, &mut done, total, &mut on_progress)?;
  Ok(())
}

fn download_ffmpeg_direct_sync(window: tauri::Window) -> Result<String, String> {
  if !cfg!(windows) {
    return Err("FFmpeg download is only supported on Windows.".to_string());
  }

  emit_ffmpeg_install_progress(&window, "start", "Preparing FFmpeg download…", None, None, None);

  // Prefer per-user location to avoid admin requirements.
  let base_dir = if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
    PathBuf::from(local_app_data).join("Clip Wave")
  } else if let Ok(exe) = env::current_exe() {
    exe.parent()
      .ok_or_else(|| "Failed to get app directory".to_string())?
      .to_path_buf()
  } else {
    return Err("Failed to determine installation directory".to_string());
  };

  let final_bin_dir = base_dir.join("bin");
  if looks_like_ffmpeg_bin_dir(&final_bin_dir) {
    emit_ffmpeg_install_progress(&window, "done", "FFmpeg is already installed.", Some(1.0), None, None);
    return Ok(final_bin_dir.to_string_lossy().to_string());
  }

  fs::create_dir_all(&base_dir).map_err(|e| format!("Failed to create directory: {e}"))?;

  // Download FFmpeg essentials (latest release)
  let url = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";

  emit_ffmpeg_install_progress(&window, "download", "Downloading FFmpeg…", None, None, None);

  let mut response = reqwest::blocking::get(url)
    .and_then(|r| r.error_for_status())
    .map_err(|e| format!("Failed to download FFmpeg: {e}"))?;

  let total_bytes = response.content_length();

  let zip_path = base_dir.join("ffmpeg-essentials.zip");
  let extract_root = base_dir.join("ffmpeg-extract");
  if extract_root.exists() {
    let _ = fs::remove_dir_all(&extract_root);
  }
  fs::create_dir_all(&extract_root).map_err(|e| format!("Failed to create directory: {e}"))?;

  // Stream download to disk to avoid holding ~120MB in memory.
  let mut zip_file = fs::File::create(&zip_path).map_err(|e| format!("Failed to create zip file: {e}"))?;
  let mut buf = vec![0u8; 256 * 1024];
  let mut downloaded: u64 = 0;
  let mut last_emit = std::time::Instant::now();
  loop {
    let read = response
      .read(&mut buf)
      .map_err(|e| format!("Failed to download FFmpeg: {e}"))?;
    if read == 0 {
      break;
    }
    zip_file
      .write_all(&buf[..read])
      .map_err(|e| format!("Failed to write zip file: {e}"))?;
    downloaded += read as u64;

    if last_emit.elapsed().as_millis() >= 250 {
      last_emit = std::time::Instant::now();
      let progress = total_bytes.and_then(|t| if t > 0 { Some(downloaded as f64 / t as f64) } else { None });
      emit_ffmpeg_install_progress(
        &window,
        "download",
        "Downloading FFmpeg…",
        progress,
        Some(downloaded),
        total_bytes,
      );
    }
  }

  emit_ffmpeg_install_progress(
    &window,
    "download",
    "Download complete.",
    Some(1.0),
    Some(downloaded),
    total_bytes,
  );

  // Extract ZIP
  emit_ffmpeg_install_progress(&window, "extract", "Extracting FFmpeg…", None, None, None);

  let file = fs::File::open(&zip_path).map_err(|e| format!("Failed to open zip file: {e}"))?;

  let mut archive = zip::ZipArchive::new(file)
    .map_err(|e| format!("Failed to read zip archive: {e}"))?;

  let total_entries = archive.len().max(1) as u64;
  let mut extracted: u64 = 0;
  let mut last_extract_emit = std::time::Instant::now();
  for i in 0..archive.len() {
    let mut file = archive.by_index(i)
      .map_err(|e| format!("Failed to read zip entry: {e}"))?;

    let outpath = match file.enclosed_name() {
      Some(path) => extract_root.join(path),
      None => continue,
    };

    if file.name().ends_with('/') {
      fs::create_dir_all(&outpath)
        .map_err(|e| format!("Failed to create directory: {e}"))?;
    } else {
      if let Some(p) = outpath.parent() {
        if !p.exists() {
          fs::create_dir_all(p)
            .map_err(|e| format!("Failed to create parent directory: {e}"))?;
        }
      }
      let mut outfile = fs::File::create(&outpath)
        .map_err(|e| format!("Failed to create file: {e}"))?;
      std::io::copy(&mut file, &mut outfile)
        .map_err(|e| format!("Failed to extract file: {e}"))?;
    }

    extracted += 1;
    if last_extract_emit.elapsed().as_millis() >= 250 {
      last_extract_emit = std::time::Instant::now();
      emit_ffmpeg_install_progress(
        &window,
        "extract",
        "Extracting FFmpeg…",
        Some(extracted as f64 / total_entries as f64),
        Some(extracted),
        Some(total_entries),
      );
    }
  }

  emit_ffmpeg_install_progress(&window, "extract", "Extraction complete.", Some(1.0), Some(total_entries), Some(total_entries));

  // Clean up zip file
  let _ = fs::remove_file(&zip_path);

  // Find the bin directory in the extracted files
  // FFmpeg essentials extracts to ffmpeg-X.X.X-essentials_build/bin
  let mut bin_dir: Option<PathBuf> = None;

  if let Ok(entries) = fs::read_dir(&extract_root) {
    for entry in entries.flatten() {
      let path = entry.path();
      if path.is_dir() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("ffmpeg-") && name.contains("essentials") {
          let candidate = path.join("bin");
          if looks_like_ffmpeg_bin_dir(&candidate) {
            bin_dir = Some(candidate);
            break;
          }
        }
      }
    }
  }

  if let Some(found_bin) = bin_dir {
    emit_ffmpeg_install_progress(&window, "install", "Installing FFmpeg…", None, None, None);

    // Replace <base>/bin with extracted bin, then clean up the temporary extraction.
    if final_bin_dir.exists() {
      let _ = fs::remove_dir_all(&final_bin_dir);
    }
    // `rename` can fail due to AV/locks/cross-device moves; copy instead.
    let mut last_install_emit = std::time::Instant::now();
    copy_dir_recursive_with_progress(&found_bin, &final_bin_dir, |done, total| {
      if last_install_emit.elapsed().as_millis() >= 250 {
        last_install_emit = std::time::Instant::now();
        emit_ffmpeg_install_progress(
          &window,
          "install",
          "Installing FFmpeg…",
          Some(done as f64 / total as f64),
          Some(done),
          Some(total),
        );
      }
    })
    .map_err(|e| format!("Failed to copy bin directory: {e}"))?;

    let _ = fs::remove_dir_all(&extract_root);

    if looks_like_ffmpeg_bin_dir(&final_bin_dir) {
      emit_ffmpeg_install_progress(&window, "done", "FFmpeg installed.", Some(1.0), None, None);
      Ok(final_bin_dir.to_string_lossy().to_string())
    } else {
      Err("FFmpeg extraction completed but bin directory is missing ffmpeg.exe/ffprobe.exe".to_string())
    }
  } else {
    Err("Failed to find FFmpeg bin directory in extracted files".to_string())
  }
}

#[cfg(debug_assertions)]
fn prewarm_ffprobe() {
  // Dev-only: optional warmup during development.
  std::thread::spawn(|| {
    if let Some(detected_dir) = auto_detect_ffmpeg_bin_dir() {
      let dir_string = detected_dir.to_string_lossy().to_string();
      let (_, ffprobe, _) = resolve_ffmpeg_binaries_with_fallback(&dir_string);
      let mut cmd = Command::new(&ffprobe);
      apply_no_window(&mut cmd);
      let _ = cmd
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output();
    } else {
      let mut cmd = Command::new("ffprobe");
      apply_no_window(&mut cmd);
      let _ = cmd
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output();
    }
  });
}

#[cfg(not(debug_assertions))]
fn prewarm_ffprobe() {}

fn main() {
  prewarm_ffprobe();

  tauri::Builder::default()
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      // Dynamically size the window to 90% of the monitor height
      if let Some(window) = app.get_webview_window("main") {
        if let Some(monitor) = window.current_monitor().unwrap_or(None) {
          let screen_h = monitor.size().height as f64 / monitor.scale_factor();
          let target_h = (screen_h * 0.90).min(1050.0).max(600.0);
          let _ = window.set_size(tauri::LogicalSize::new(950.0, target_h));
        }
      }

      Ok(())
    })
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_opener::init())
    .invoke_handler(tauri::generate_handler![
      detect_ffmpeg_bin_dir,
      check_ffmpeg,
      check_winget,
      install_ffmpeg_winget,
      lossless_preflight,
      warm_ffprobe,
      probe_duration,
      probe_tracks,
      probe_subtitles,
      probe_media,
      trim_media,
      add_defender_exclusion,
      check_defender_exclusion_needed,
      get_app_dir,
      download_ffmpeg_direct
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
