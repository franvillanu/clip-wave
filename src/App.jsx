import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import clipwaveLogo from './assets/clipwave-logo.png'

function parseHhMmSs(value) {
  // Accept both hh:mm:ss and hh:mm:ss.milliseconds formats
  if (!/^\d+:\d{2}:\d{2}(\.\d{1,3})?$/.test(value)) {
    return { ok: false, error: 'Time must be in format hh:mm:ss or hh:mm:ss.milliseconds' }
  }
  const [hRaw, mRaw, sRaw] = value.split(':')
  const hours = Number(hRaw)
  const minutes = Number(mRaw)
  const seconds = Number(sRaw)  // parseFloat handles decimals
  if (
    Number.isNaN(hours) ||
    Number.isNaN(minutes) ||
    Number.isNaN(seconds) ||
    minutes < 0 ||
    minutes >= 60 ||
    seconds < 0 ||
    seconds >= 60
  ) {
    return { ok: false, error: 'Minutes and seconds must be < 60' }
  }
  return { ok: true, seconds: hours * 3600 + minutes * 60 + seconds }
}

function formatSecondsToHhMmSs(totalSeconds) {
  if (typeof totalSeconds !== 'number' || !Number.isFinite(totalSeconds) || totalSeconds < 0) {
    return ''
  }
  const rounded = Math.floor(totalSeconds)
  const hours = Math.floor(rounded / 3600)
  const minutes = Math.floor((rounded % 3600) / 60)
  const seconds = rounded % 60
  const mm = String(minutes).padStart(2, '0')
  const ss = String(seconds).padStart(2, '0')
  return `${hours}:${mm}:${ss}`
}

function formatSecondsToHhMmSsWithMillis(totalSeconds) {
  if (typeof totalSeconds !== 'number' || !Number.isFinite(totalSeconds) || totalSeconds < 0) {
    return ''
  }
  const hours = Math.floor(totalSeconds / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  const seconds = totalSeconds % 60
  const hh = String(hours).padStart(2, '0')
  const mm = String(minutes).padStart(2, '0')
  const ss = seconds.toFixed(3).padStart(6, '0')
  return `${hh}:${mm}:${ss}`
}

function ensureMaskedTime(value) {
  const digits = String(value ?? '')
    .replace(/\D/g, '')
    .slice(0, 6)
    .padEnd(6, '0')
  return `${digits.slice(0, 2)}:${digits.slice(2, 4)}:${digits.slice(4, 6)}`
}

function digitIndexFromCaretPos(pos) {
  if (pos <= 0) return 0
  if (pos <= 1) return 1
  if (pos <= 3) return 2
  if (pos <= 4) return 3
  if (pos <= 6) return 4
  return 5
}

function caretPosFromDigitIndex(digitIndex) {
  switch (digitIndex) {
    case 0:
      return 0
    case 1:
      return 1
    case 2:
      return 3
    case 3:
      return 4
    case 4:
      return 6
    default:
      return 7
  }
}

function segmentRangeFromDigitIndex(digitIndex) {
  if (digitIndex <= 1) return { start: 0, end: 2 }
  if (digitIndex <= 3) return { start: 3, end: 5 }
  return { start: 6, end: 8 }
}

function setDigitAtMaskedTime(value, digitIndex, digitChar) {
  const masked = ensureMaskedTime(value)
  const chars = masked.split('')
  const pos = caretPosFromDigitIndex(digitIndex)
  chars[pos] = digitChar
  return chars.join('')
}

function isDigitKey(e) {
  return e.key.length === 1 && e.key >= '0' && e.key <= '9'
}

function TimeInput({ value, onChange, disabled, ariaLabel, className, maxSeconds }) {
  const maskedValue = ensureMaskedTime(value)
  const pointerDownRef = useRef(false)

  function clampTotalSeconds(totalSeconds) {
    const absoluteMax = 99 * 3600 + 59 * 60 + 59
    const effectiveMax = typeof maxSeconds === 'number' && maxSeconds > 0 ? Math.min(maxSeconds, absoluteMax) : absoluteMax
    return Math.min(effectiveMax, Math.max(0, totalSeconds))
  }

  function parseMaskedToSeconds(masked) {
    const [hh, mm, ss] = String(masked).split(':').map((v) => Number(v))
    return (Number(hh) || 0) * 3600 + (Number(mm) || 0) * 60 + (Number(ss) || 0)
  }

  function secondsToMasked(totalSeconds) {
    const clamped = clampTotalSeconds(totalSeconds)
    const hh = Math.floor(clamped / 3600)
    const mm = Math.floor((clamped % 3600) / 60)
    const ss = clamped % 60
    return `${String(hh).padStart(2, '0')}:${String(mm).padStart(2, '0')}:${String(ss).padStart(2, '0')}`
  }

  function setSelection(input, start, end) {
    requestAnimationFrame(() => {
      try {
        input.setSelectionRange(start, end)
      } catch (_) {}
    })
  }

  function caretPosFromClick(input, clientX) {
    const rect = input.getBoundingClientRect()
    const style = window.getComputedStyle(input)
    const paddingLeft = parseFloat(style.paddingLeft) || 0
    const paddingRight = parseFloat(style.paddingRight) || 0
    const contentWidth = rect.width - paddingLeft - paddingRight
    const letterSpacing = Number.parseFloat(style.letterSpacing)
    const spacing = Number.isFinite(letterSpacing) ? letterSpacing : 0
    const text = input.value ?? ''

    const canvas = document.createElement('canvas')
    const ctx = canvas.getContext('2d')
    if (!ctx) return input.selectionStart ?? 0

    ctx.font = `${style.fontStyle} ${style.fontVariant} ${style.fontWeight} ${style.fontSize} / ${style.lineHeight} ${style.fontFamily}`

    const fullTextWidth = ctx.measureText(text).width + spacing * Math.max(0, text.length - 1)
    let startOffset = 0
    if (style.textAlign === 'center') {
      startOffset = (contentWidth - fullTextWidth) / 2
    } else if (style.textAlign === 'right' || style.textAlign === 'end') {
      startOffset = contentWidth - fullTextWidth
    }

    const rawX = clientX - rect.left - paddingLeft - startOffset
    const x = Math.max(0, Math.min(fullTextWidth, rawX))

    let bestIndex = 0
    let bestDist = Number.POSITIVE_INFINITY
    for (let i = 0; i <= text.length; i += 1) {
      const width = ctx.measureText(text.slice(0, i)).width + spacing * Math.max(0, i - 1)
      const dist = Math.abs(x - width)
      if (dist < bestDist) {
        bestDist = dist
        bestIndex = i
      }
    }

    return bestIndex
  }

  function handleFocus(e) {
    if (pointerDownRef.current) return
    const input = e.currentTarget
    const range = segmentRangeFromDigitIndex(5)
    setSelection(input, range.start, range.end)
  }

  function handlePointerDown() {
    pointerDownRef.current = true
  }

  function handlePointerUp(e) {
    pointerDownRef.current = false
    const input = e.currentTarget
    requestAnimationFrame(() => {
      const pos = caretPosFromClick(input, e.clientX)
      const di = pos === 2 ? 1 : pos === 5 ? 3 : digitIndexFromCaretPos(pos)
      const range = segmentRangeFromDigitIndex(di)
      setSelection(input, range.start, range.end)
    })
  }

  function handleKeyDown(e) {
    const input = e.currentTarget
    const selStart = input.selectionStart ?? 0
    const selEnd = input.selectionEnd ?? 0

    const hasSelection = selEnd > selStart
    const caretPos = selStart
    const digitIndex = digitIndexFromCaretPos(caretPos)
    const range = segmentRangeFromDigitIndex(digitIndex)

    if (e.key === 'Tab') return

    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault()
      const nextDigitIndex =
        e.key === 'ArrowLeft' ? Math.max(0, digitIndex - 1) : Math.min(5, digitIndex + 1)
      const nextRange = segmentRangeFromDigitIndex(nextDigitIndex)
      setSelection(input, nextRange.start, nextRange.end)
      return
    }

    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault()
      const delta = e.key === 'ArrowUp' ? 1 : -1
      const seg = digitIndex <= 1 ? 'h' : digitIndex <= 3 ? 'm' : 's'
      const currentSeconds = parseMaskedToSeconds(maskedValue)
      const nextSeconds =
        seg === 'h' ? currentSeconds + delta * 3600 : seg === 'm' ? currentSeconds + delta * 60 : currentSeconds + delta
      const updated = secondsToMasked(nextSeconds)
      onChange(updated)
      setSelection(input, range.start, range.end)
      return
    }

    if (e.key === 'Backspace' || e.key === 'Delete') {
      e.preventDefault()
      const targetDigitIndex =
        e.key === 'Backspace' ? Math.max(0, digitIndex - (hasSelection ? 0 : 1)) : digitIndex
      const updated = setDigitAtMaskedTime(maskedValue, targetDigitIndex, '0')
      onChange(updated)
      const nextRange = segmentRangeFromDigitIndex(targetDigitIndex)
      setSelection(input, nextRange.start, nextRange.end)
      return
    }

    if (isDigitKey(e)) {
      e.preventDefault()
      const digit = e.key

      if (hasSelection) {
        const fullSegmentSelected = selStart <= range.start && selEnd >= range.end
        const secondDigitSelected = selStart === range.start + 1 && selEnd === range.start + 2

        if (fullSegmentSelected) {
          const target = digitIndexFromCaretPos(range.start)
          const updated = setDigitAtMaskedTime(maskedValue, target, digit)
          onChange(updated)
          setSelection(input, range.start + 1, range.start + 2)
          return
        }

        if (secondDigitSelected) {
          const target = digitIndexFromCaretPos(range.start + 1)
          const updated = setDigitAtMaskedTime(maskedValue, target, digit)
          onChange(updated)
          const nextDigitIndex = Math.min(5, target + 1)
          const nextRange = segmentRangeFromDigitIndex(nextDigitIndex)
          setSelection(input, nextRange.start, nextRange.end)
          return
        }

        const target = digitIndexFromCaretPos(caretPos)
        const updated = setDigitAtMaskedTime(maskedValue, target, digit)
        onChange(updated)
        const nextDigitIndex = Math.min(5, target + 1)
        const nextRange = segmentRangeFromDigitIndex(nextDigitIndex)
        setSelection(input, nextRange.start, nextRange.end)
        return
      }

      const updated = setDigitAtMaskedTime(maskedValue, digitIndex, digit)
      onChange(updated)
      const nextDigitIndex = Math.min(5, digitIndex + 1)
      const nextRange = segmentRangeFromDigitIndex(nextDigitIndex)
      setSelection(input, nextRange.start, nextRange.end)
      return
    }

    if (e.key === ':' || e.key === ' ') {
      e.preventDefault()
      const nextDigitIndex = digitIndex <= 1 ? 2 : digitIndex <= 3 ? 4 : 5
      const nextRange = segmentRangeFromDigitIndex(nextDigitIndex)
      setSelection(input, nextRange.start, nextRange.end)
      return
    }

    if (e.ctrlKey || e.metaKey) return

    e.preventDefault()
  }

  function handlePaste(e) {
    const text = e.clipboardData.getData('text') ?? ''
    const digits = text.replace(/\D/g, '').slice(0, 6)
    if (!digits) return
    e.preventDefault()

    const input = e.currentTarget
    const caretPos = input.selectionStart ?? 0
    let di = digitIndexFromCaretPos(caretPos)
    let updated = maskedValue
    for (const d of digits) {
      updated = setDigitAtMaskedTime(updated, di, d)
      di = Math.min(5, di + 1)
    }
    onChange(updated)
    const nextRange = segmentRangeFromDigitIndex(di)
    setSelection(input, nextRange.start, nextRange.end)
  }

  return (
    <input
      className={['vt-input', className || ''].filter(Boolean).join(' ')}
      value={maskedValue}
      onChange={() => {}}
      onKeyDown={handleKeyDown}
      onFocus={handleFocus}
      onPointerDown={handlePointerDown}
      onPointerUp={handlePointerUp}
      onPaste={handlePaste}
      disabled={disabled}
      inputMode="numeric"
      aria-label={ariaLabel}
      spellCheck={false}
    />
  )
}

function getLanguageName(code) {
  const langMap = {
    'en': 'English',
    'eng': 'English',
    'es': 'Spanish',
    'spa': 'Spanish',
    'fr': 'French',
    'fre': 'French',
    'fra': 'French',
    'de': 'German',
    'ger': 'German',
    'deu': 'German',
    'it': 'Italian',
    'ita': 'Italian',
    'pt': 'Portuguese',
    'por': 'Portuguese',
    'ja': 'Japanese',
    'jpn': 'Japanese',
    'zh': 'Chinese',
    'chi': 'Chinese',
    'zho': 'Chinese',
    'ko': 'Korean',
    'kor': 'Korean',
    'ru': 'Russian',
    'rus': 'Russian',
    'ar': 'Arabic',
    'ara': 'Arabic',
    'hi': 'Hindi',
    'hin': 'Hindi',
    'cs': 'Czech',
    'cze': 'Czech',
    'ces': 'Czech',
    'da': 'Danish',
    'dan': 'Danish',
    'el': 'Greek',
    'gre': 'Greek',
    'ell': 'Greek',
    'fi': 'Finnish',
    'fin': 'Finnish',
    'fil': 'Filipino',
    'he': 'Hebrew',
    'heb': 'Hebrew',
    'hr': 'Croatian',
    'hrv': 'Croatian',
    'nl': 'Dutch',
    'dut': 'Dutch',
    'nld': 'Dutch',
    'no': 'Norwegian',
    'nor': 'Norwegian',
    'pl': 'Polish',
    'pol': 'Polish',
    'ro': 'Romanian',
    'rum': 'Romanian',
    'ron': 'Romanian',
    'sv': 'Swedish',
    'swe': 'Swedish',
    'th': 'Thai',
    'tha': 'Thai',
    'tr': 'Turkish',
    'tur': 'Turkish',
    'uk': 'Ukrainian',
    'ukr': 'Ukrainian',
    'vi': 'Vietnamese',
    'vie': 'Vietnamese',
    'und': 'Undefined',
  }
  const lower = String(code || '').toLowerCase().trim()
  return langMap[lower] || null
}

function audioLabel(stream) {
  const id =
    stream && typeof stream === 'object' && 'order' in stream && Number.isFinite(Number(stream.order))
      ? `a:${Number(stream.order)}`
      : `#${stream.index}`

  const langCode = stream.language || 'und'
  const langName = getLanguageName(langCode)

  const parts = [
    id,
    langCode,
    stream.codec_name || '',
    stream.channels != null ? String(stream.channels) : '',
    stream.title || '',
  ].filter((p) => p !== '')

  const label = parts.join(' ')
  return langName ? `${label} (${langName})` : label
}

function subtitleLabel(stream) {
  const langCode = stream.language || 'und'
  const langName = getLanguageName(langCode)

  const parts = [
    `#${stream.index}`,
    langCode,
    stream.codec_name || '',
    stream.title || '',
  ].filter((p) => p !== '')

  const label = parts.join(' ')
  return langName ? `${label} (${langName})` : label
}

function App() {
  const [debugLogsEnabled, setDebugLogsEnabled] = useState(() => {
    try {
      return localStorage.getItem('clipwave.debugLogsEnabled') === 'true'
    } catch {
      return false
    }
  })


  const [inputPath, setInputPath] = useState('')
  const [durationSeconds, setDurationSeconds] = useState(null)
  const [audioStreams, setAudioStreams] = useState([])
  const [selectedAudioIndex, setSelectedAudioIndex] = useState(-1)
  const [subtitleStreams, setSubtitleStreams] = useState([])
  const [selectedSubtitleIndex, setSelectedSubtitleIndex] = useState(-1)
  const [ffmpegBinDir, setFfmpegBinDir] = useState(() => {
    try {
      return localStorage.getItem('clipwave.ffmpegBinDir') || ''
    } catch {
      return ''
    }
  })
  // Default to false to force explicit detection/installation on first run.
  const [ffmpegOk, setFfmpegOk] = useState(() => {
    try {
      return localStorage.getItem('clipwave.ffmpegOk') === 'true'
    } catch {
      return false
    }
  })
  const [ffmpegOkCache, setFfmpegOkCache] = useState(() => {
    try {
      return localStorage.getItem('clipwave.ffmpegOk') === 'true'
    } catch {
      return false
    }
  })
  const [ffmpegCheckMessage, setFfmpegCheckMessage] = useState('')
  const [isCheckingFfmpeg, setIsCheckingFfmpeg] = useState(false)
  const [isDownloadingFfmpeg, setIsDownloadingFfmpeg] = useState(false)
  const [ffmpegInstallOverlayText, setFfmpegInstallOverlayText] = useState('')
  const [ffmpegInstallProgress, setFfmpegInstallProgress] = useState(null)
  const [ffmpegRequiredModalOpen, setFfmpegRequiredModalOpen] = useState(false)
  const [wingetAvailable, setWingetAvailable] = useState(true)
  const [wingetMessage, setWingetMessage] = useState('')
  const [isCheckingWinget, setIsCheckingWinget] = useState(false)
  const [inTime, setInTime] = useState('00:00:00')
  const [outTime, setOutTime] = useState('00:00:10')
  const [mode, setMode] = useState('lossless')
  const [statusLog, setStatusLog] = useState([])
  const [outputPath, setOutputPath] = useState('')
  const [busyAction, setBusyAction] = useState('idle') // idle | probing | cutting
  const [touchedIn, setTouchedIn] = useState(false)
  const [touchedOut, setTouchedOut] = useState(false)
  const [touchedAudio, setTouchedAudio] = useState(false)
  const [touchedSubs, setTouchedSubs] = useState(false)
  const [isLoadingTracks, setIsLoadingTracks] = useState(false)
  const [isLoadingSubs, setIsLoadingSubs] = useState(false)
  const [subsStatus, setSubsStatus] = useState('not_loaded') // not_loaded | loading | loaded_none | loaded
  const [losslessModal, setLosslessModal] = useState({
    open: false,
    checking: false,
    message: '',
    inTime: '',
    outTime: '',
    shiftSeconds: null,
    keyframeTime: null,
    keyframeSeconds: null,
    nextKeyframeTime: null,
    nextKeyframeDelta: null,
    nextKeyframeSeconds: null,
  })
  const [losslessTipExpanded, setLosslessTipExpanded] = useState(false)

  const statusSeqRef = useRef(0)
  const openSeqRef = useRef(0)
  const subsRequestIdRef = useRef(0)
  const ffmpegInstallPhaseRef = useRef('')
  const ffmpegInstallProgressRef = useRef({ phase: '', progress: null })
  const losslessPreflightReqRef = useRef(0)
  const skipLosslessPreflightKeyRef = useRef('')
  const isAdjustingTimeRef = useRef(false)
  const [timeAdjustToken, setTimeAdjustToken] = useState(0)
  const holdIntervalRef = useRef(null)
  const holdTimeoutRef = useRef(null)
  const ffprobeWarmRef = useRef(false)
  const readyLoggedRef = useRef(false)

  const isProbing = busyAction === 'probing'
  const isCutting = busyAction === 'cutting'
  const busy = busyAction !== 'idle'

  const durationText = useMemo(() => {
    if (durationSeconds == null) return ''
    return formatSecondsToHhMmSs(durationSeconds)
  }, [durationSeconds])

  function pushStatus(message, kind = 'info', scope = 'user') {
    const text = String(message)
    const id = ++statusSeqRef.current
    setStatusLog((prev) => {
      const next = [{ id, ts: Date.now(), kind, scope, text }, ...prev]
      return next.slice(0, 500)
    })
  }

  function logUser(message, kind = 'info') {
    pushStatus(message, kind, 'user')
  }

  function logDebug(message, kind = 'info') {
    pushStatus(message, kind, 'debug')
  }

  function formatBytes(bytes) {
    const b = Number(bytes)
    if (!Number.isFinite(b) || b <= 0) return '0 B'
    const units = ['B', 'KB', 'MB', 'GB']
    const idx = Math.min(units.length - 1, Math.floor(Math.log(b) / Math.log(1024)))
    const value = b / 1024 ** idx
    const decimals = idx === 0 ? 0 : idx === 1 ? 0 : 1
    return `${value.toFixed(decimals)} ${units[idx]}`
  }

  useEffect(() => {
    let unlisten = null
    ;(async () => {
      try {
        unlisten = await listen('ffmpeg_install_progress', (event) => {
          const payload = event?.payload && typeof event.payload === 'object' ? event.payload : {}
          const phase = String(payload?.phase || '')
          const message = String(payload?.message || '').trim()
          const progressRaw = payload?.progress
          let progress = typeof progressRaw === 'number' && Number.isFinite(progressRaw)
            ? Math.max(0, Math.min(1, progressRaw))
            : null
          const bytesDone = typeof payload?.bytes_done === 'number' ? payload.bytes_done : null
          const bytesTotal = typeof payload?.bytes_total === 'number' ? payload.bytes_total : null

          const last = ffmpegInstallProgressRef.current
          if (phase === last.phase) {
            if (typeof last.progress === 'number') {
              if (typeof progress === 'number' && progress < last.progress) {
                progress = last.progress
              }
              if (progress == null) {
                progress = last.progress
              }
            }
          } else if (typeof progress === 'number' && progress <= 0) {
            progress = null
          }
          ffmpegInstallProgressRef.current = {
            phase,
            progress: typeof progress === 'number' ? progress : null,
          }

          if (message) setFfmpegInstallOverlayText(message)
          setFfmpegInstallProgress({
            phase,
            progress,
            bytesDone,
            bytesTotal,
          })

          if (phase && phase !== ffmpegInstallPhaseRef.current) {
            ffmpegInstallPhaseRef.current = phase
            if (message) {
              const kind = phase === 'done' ? 'success' : phase === 'error' ? 'error' : 'info'
              logUser(message, kind)
            }
          }
        })
      } catch {
        // ignore
      }
    })()

    return () => {
      try {
        if (typeof unlisten === 'function') unlisten()
      } catch {
        // ignore
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  function handleClearLogs() {
    setStatusLog([])
    statusSeqRef.current = 0
  }

  function handleRefreshApp() {
    try {
      window.location.reload()
    } catch {
      // ignore
    }
  }

  const wingetCommand =
    'winget install -e --id Gyan.FFmpeg --accept-source-agreements --accept-package-agreements'

  async function runCheckFfmpeg(nextDir) {
    setIsCheckingFfmpeg(true)
    try {
      const result = await invoke('check_ffmpeg', { ffmpegBinDir: nextDir ?? ffmpegBinDir })
      if (result && typeof result === 'object' && 'ok' in result) {
        const ok = Boolean(result.ok)
        setFfmpegOk(ok)
        setFfmpegCheckMessage(String(result.message || ''))
        const usedDir = result.ffmpeg_bin_dir_used
        if (!ffmpegBinDir && usedDir) {
          setFfmpegBinDir(String(usedDir))
        }
        if (!ok) {
          await runCheckWinget()
        }
      } else {
        setFfmpegOk(true)
        setFfmpegCheckMessage('')
      }
    } catch (e) {
      setFfmpegOk(false)
      setFfmpegCheckMessage(String(e?.message || e))
      await runCheckWinget()
    } finally {
      setIsCheckingFfmpeg(false)
    }
  }

  async function runCheckWinget() {
    if (isCheckingWinget) return
    setIsCheckingWinget(true)
    try {
      const result = await invoke('check_winget')
      if (result && typeof result === 'object' && 'available' in result) {
        setWingetAvailable(Boolean(result.available))
        setWingetMessage(String(result.message || ''))
      } else {
        setWingetAvailable(true)
        setWingetMessage('')
      }
    } catch (e) {
      setWingetAvailable(false)
      setWingetMessage(String(e?.message || e))
    } finally {
      setIsCheckingWinget(false)
    }
  }

  async function handleInstallFfmpeg() {
    try {
      if (!wingetAvailable) {
        pushStatus(wingetMessage || 'WinGet is not available on this system.', 'error')
        return
      }
      pushStatus('Installing FFmpeg... Click Re-check when PowerShell finishes.', 'info')
      await invoke('install_ffmpeg_winget')
    } catch (e) {
      pushStatus(String(e?.message || e), 'error')
    }
  }

  function baseDirFromBin(binDir) {
    const text = String(binDir || '')
    return text.replace(/[\\/]+bin[\\/]*$/i, '')
  }

  async function handleDownloadFfmpegDirect() {
    if (isDownloadingFfmpeg) return
    setIsDownloadingFfmpeg(true)
    try {
      setFfmpegInstallOverlayText('Installing FFmpeg…')
      setFfmpegInstallProgress(null)
      ffmpegInstallPhaseRef.current = ''
      logUser('Installing FFmpeg (~120MB)…', 'info')
      logUser('Please keep Clip Wave open until it finishes.', 'info')
      const binDir = await invoke('download_ffmpeg_direct')
      if (typeof binDir !== 'string' || !binDir) throw new Error('FFmpeg download returned an invalid path.')

      setFfmpegBinDir(binDir)
      await runCheckFfmpeg(binDir)
      logDebug(`FFmpeg installed to ${binDir}`, 'success')
    } catch (e) {
      logUser('FFmpeg download failed.', 'error')
      logDebug(`FFmpeg download failed: ${String(e?.message || e)}`, 'error')
    } finally {
      setIsDownloadingFfmpeg(false)
    }
  }

  async function ensureWarmFfprobe(binDir) {
    if (!binDir) return
    if (ffprobeWarmRef.current) return
    try {
      logDebug('Warming up ffprobe…', 'info')
      const w = await invoke('warm_ffprobe', { ffmpegBinDir: binDir })
      ffprobeWarmRef.current = true
      if (w && typeof w === 'object' && 'ms' in w) logDebug(`ffprobe warmup: ${Number(w.ms).toFixed(1)}ms`, 'success')
    } catch (_) {
      // best-effort; if warmup fails we still allow probing to proceed
    }
  }

  async function handleAddDefenderExclusionForFfmpeg() {
    try {
      if (!ffmpegBinDir) {
        logUser('Set up FFmpeg first (Download or choose a bin folder).', 'error')
        return
      }
      const baseDir = baseDirFromBin(ffmpegBinDir) || ffmpegBinDir
      logUser('Requesting Windows Defender exclusion (UAC prompt)…', 'info')
      await invoke('add_defender_exclusion', { path: baseDir })
      logUser('Windows Defender exclusion requested.', 'success')
    } catch (e) {
      logUser('Failed to add Defender exclusion.', 'error')
      logDebug(`Failed to add Defender exclusion: ${String(e?.message || e)}`, 'error')
    }
  }


  function secondsToTime(totalSeconds) {
    const s = Math.max(0, Math.floor(Number(totalSeconds) || 0))
    const hh = Math.floor(s / 3600)
    const mm = Math.floor((s % 3600) / 60)
    const ss = s % 60
    return `${String(hh).padStart(2, '0')}:${String(mm).padStart(2, '0')}:${String(ss).padStart(2, '0')}`
  }

  function secondsToTimeRounded(totalSeconds) {
    return secondsToTime(Math.round(Number(totalSeconds) || 0))
  }

  function formatDuration(totalSeconds) {
    const abs = Math.abs(Number(totalSeconds) || 0)
    const roundedSeconds = Math.round(abs)
    const base = secondsToTime(roundedSeconds)
    if (roundedSeconds === 0 && abs > 0) return `${base} (${abs.toFixed(3)}s)`
    return base
  }

  function isEnglishOrSpanish(language) {
    const l = String(language || '').trim().toLowerCase()
    return l === 'en' || l === 'eng' || l.startsWith('en-') || l === 'es' || l === 'spa' || l.startsWith('es-')
  }

  function filterEnglishSpanish(streams) {
    if (!Array.isArray(streams) || streams.length === 0) return []
    const filtered = streams.filter((s) => isEnglishOrSpanish(s.language))
    return filtered.length ? filtered : streams
  }

  function pickPreferredStreamIndex(streams) {
    if (!Array.isArray(streams) || streams.length === 0) return -1
    // Simply return the first audio track (the one that would play natively)
    return streams[0].order ?? streams[0].index ?? -1
  }

  function pickPreferredSubtitleIndex(streams) {
    if (!Array.isArray(streams) || streams.length === 0) return -1
    const english = streams.find((s) => String(s.language || '').toLowerCase().startsWith('en'))
    return (english?.index ?? streams[0].index) ?? -1
  }

  useEffect(() => {
    try {
      localStorage.setItem('clipwave.debugLogsEnabled', debugLogsEnabled ? 'true' : 'false')
    } catch {
      // ignore
    }
  }, [debugLogsEnabled])

  useEffect(() => {
    try {
      localStorage.setItem('clipwave.ffmpegBinDir', ffmpegBinDir || '')
    } catch {
      // ignore
    }
  }, [ffmpegBinDir])

  useEffect(() => {
    // Auto-check only when we have a previously saved bin dir (fast path, no PATH scanning).
    if (ffmpegBinDir) {
      void runCheckFfmpeg(ffmpegBinDir)
      return
    }

    if (ffmpegOkCache) {
      setFfmpegRequiredModalOpen(false)
      return
    }

    // First run (no bin dir saved): require explicit install/check flow.
    setFfmpegOk(false)
    try {
      const seen = localStorage.getItem('clipwave.ffmpegRequiredPromptSeen') === 'true'
      setFfmpegRequiredModalOpen(!seen)
    } catch {
      setFfmpegRequiredModalOpen(true)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    // Ensure the log panel is never empty (and production mode shows something immediately).
    if (readyLoggedRef.current) return
    readyLoggedRef.current = true
    pushStatus('Ready.', 'info', 'user')
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    // Once FFmpeg is detected, persist "prompt seen" so it doesn't reappear.
    if (!ffmpegOk) return
    try {
      localStorage.setItem('clipwave.ffmpegRequiredPromptSeen', 'true')
    } catch {
      // ignore
    }
    setFfmpegRequiredModalOpen(false)
  }, [ffmpegOk])

  useEffect(() => {
    try {
      localStorage.setItem('clipwave.ffmpegOk', ffmpegOk ? 'true' : 'false')
    } catch {
      // ignore
    }
    setFfmpegOkCache(ffmpegOk)
  }, [ffmpegOk])

  const inParsed = useMemo(() => parseHhMmSs(inTime), [inTime])
  const outParsed = useMemo(() => parseHhMmSs(outTime), [outTime])
  const rangeOk = inParsed.ok && outParsed.ok && outParsed.seconds > inParsed.seconds
  const inError = inParsed.ok ? '' : inParsed.error
  const outError = outParsed.ok ? '' : outParsed.error
  const rangeError =
    inParsed.ok && outParsed.ok && outParsed.seconds <= inParsed.seconds ? 'OUT must be greater than IN.' : ''

  const subsBlocksCut = Boolean(isLoadingSubs) && selectedSubtitleIndex >= 0
  const canCut =
    Boolean(inputPath) && !isProbing && !isCutting && !isLoadingTracks && !subsBlocksCut && rangeOk && ffmpegOk

  const visibleStatusLog = useMemo(() => {
    if (debugLogsEnabled) return statusLog
    return statusLog.filter((l) => l && typeof l === 'object' && l.scope === 'user')
  }, [debugLogsEnabled, statusLog])

  // Intentionally do not auto-detect FFmpeg on startup to avoid any first-frame freezes.

  useEffect(() => {
    return () => {
      if (holdTimeoutRef.current) clearTimeout(holdTimeoutRef.current)
      if (holdIntervalRef.current) clearInterval(holdIntervalRef.current)
    }
  }, [])

  useEffect(() => {
    function onKeyDown(e) {
      const active = document.activeElement
      const tag = active?.tagName?.toLowerCase?.() || ''

      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'o') {
        e.preventDefault()
        if (!isCutting && !isProbing) handleOpenVideo()
        return
      }

      if (e.key === 'Escape') {
        e.preventDefault()
        setStatusLog([])
        return
      }

      if (e.key === 'Enter') {
        if (tag === 'select') return
        if (!canCut) return
        e.preventDefault()
        handleCut()
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [canCut, isCutting, isProbing, inputPath, inTime, outTime, mode, selectedAudioIndex, selectedSubtitleIndex, ffmpegBinDir])

  function losslessPreflightKey(path, inT, outT, binDir) {
    return `${String(path || '')}|${String(inT || '')}|${String(outT || '')}|${String(binDir || '')}`
  }

  async function fetchSubsAsync(path, openSeq) {
    if (!path) return
    const requestId = ++subsRequestIdRef.current

    setIsLoadingSubs(true)
    setSubsStatus('loading')
    logUser('Loading subtitles…', 'info')
    logDebug(`Loading subtitles | request=${requestId}`, 'info')

    try {
      const subsResult = await invoke('probe_subtitles', { inputPath: path, ffmpegBinDir })
      if (openSeqRef.current !== openSeq || subsRequestIdRef.current !== requestId) {
        logDebug(`Subtitles result ignored (stale) | request=${requestId}`, 'info')
        return
      }

      const allSubs = Array.isArray(subsResult?.subtitle_streams) ? subsResult.subtitle_streams : []
      const subs = allSubs
      const timing = subsResult?.timing_ms
      if (timing && typeof timing === 'object') {
        logDebug(
          `⏱ (Subs) Probe: ${Number(timing.ffprobe_ms || 0).toFixed(1)}ms | Total: ${Number(timing.total_ms || 0).toFixed(1)}ms | Cache: ${timing.cache_hit ? 'hit' : 'miss'}`,
          'info'
        )
        if (timing.cache_hit) logUser('Subtitles cache hit.', 'success')
      }

      setSubtitleStreams(subs)
      if (subs.length === 0) {
        setSubsStatus('loaded_none')
        setSelectedSubtitleIndex(-1)
        logUser('No subtitles found.', 'info')
      } else {
        setSubsStatus('loaded')
        if (!touchedSubs) {
          const pick = pickPreferredSubtitleIndex(subs)
          if (pick >= 0) setSelectedSubtitleIndex(pick)
        }
        logUser(`Loaded ${subs.length} subtitle track(s).`, 'success')
      }
    } catch (e) {
      if (openSeqRef.current !== openSeq || subsRequestIdRef.current !== requestId) return
      setSubsStatus('loaded_none')
      setSubtitleStreams([])
      setSelectedSubtitleIndex(-1)
      logUser('Subtitles unavailable.', 'error')
      logDebug(`Subtitles probe failed: ${String(e?.message || e)}`, 'error')
    } finally {
      if (openSeqRef.current === openSeq && subsRequestIdRef.current === requestId) {
        setIsLoadingSubs(false)
      }
    }
  }

  async function handleOpenVideo() {
    try {
      setBusyAction('probing')
      logUser('Opening file dialog…', 'info')
      if (isDownloadingFfmpeg) {
        logUser('Please wait for FFmpeg installation to finish.', 'info')
        return
      }
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name: 'Video',
            extensions: ['mkv', 'mp4', 'mov', 'avi', 'webm', 'm4v'],
          },
        ],
      })

      if (!selected) {
        logUser('Open canceled.', 'info')
        return
      }

      const path = Array.isArray(selected) ? selected[0] : selected
      if (!path) {
        logUser('Open canceled.', 'info')
        return
      }

      const openSeq = ++openSeqRef.current
      subsRequestIdRef.current += 1
      setInputPath(path)
      setOutputPath('')
      setDurationSeconds(null)
      setAudioStreams([])
      setSelectedAudioIndex(-1)
      setTouchedAudio(false)
      setSubtitleStreams([])
      setSelectedSubtitleIndex(-1)
      setTouchedSubs(false)
      setIsLoadingSubs(false)
      setSubtitleStreams([])

      setSubsStatus('loading')

      logUser('Probing duration…', 'info')
      const durationResult = await invoke('probe_duration', {
        inputPath: path,
        ffmpegBinDir,
      })

      if (durationResult?.timing_ms) {
        const t = durationResult.timing_ms
        const engine = String(durationResult?.ffprobe_runner || '').toLowerCase() === 'mf' ? 'MF' : 'FFprobe'
        logDebug(
          `⏱ (Duration) Validation: ${t.validation_ms.toFixed(1)}ms | Resolve: ${t.resolve_binaries_ms.toFixed(1)}ms | ${engine}: ${t.ffprobe_execution_ms.toFixed(1)}ms | Total: ${t.total_ms.toFixed(1)}ms`,
          'info'
        )
        if (t.ffprobe_execution_ms > 500 && durationResult?.debug) {
          const d = durationResult.debug
          logDebug(
            `diag(duration): program=${d.program_exists ? 'exists' : 'missing'} | exit=${d.exit_code ?? '?'} | success=${Boolean(d.success)} | out=${d.stdout_len}B | err=${d.stderr_len}B`,
            'info'
          )
          if (d.stderr_head) logDebug(`diag(duration) stderr: ${String(d.stderr_head).replace(/\\s+/g, ' ').trim()}`, 'info')
        }
      }

      const duration = typeof durationResult?.duration_seconds === 'number' ? durationResult.duration_seconds : null
      const usedBin = typeof durationResult?.ffmpeg_bin_dir_used === 'string' ? durationResult.ffmpeg_bin_dir_used : ''
      setDurationSeconds(duration)
      if (!ffmpegBinDir && usedBin) setFfmpegBinDir(usedBin)

      if (!touchedOut) {
        if (typeof duration === 'number' && Number.isFinite(duration) && duration > 0) {
          // Set OUT to full video duration by default
          setOutTime(secondsToTime(Math.floor(duration)))
        } else {
          setOutTime('00:00:10')
        }
      }

      // Tracks probe can be slow; run in background and keep UI responsive.
      setBusyAction('idle')
      setIsLoadingTracks(true)
      logUser('Loading audio tracks…', 'info')
      ;(async () => {
        try {
          const tracks = await invoke('probe_tracks', { inputPath: path, ffmpegBinDir })
          if (openSeqRef.current !== openSeq) return
          const allAudio = Array.isArray(tracks?.audio_streams) ? tracks.audio_streams : []
          const streams = allAudio
          setAudioStreams(streams)

          const audioPick = pickPreferredStreamIndex(streams)
          setSelectedAudioIndex(audioPick >= 0 ? audioPick : -1)

          const tt = tracks?.timing_ms
          if (tt && typeof tt.total_ms === 'number') {
            const engine = String(tracks?.ffprobe_runner || '').toLowerCase() === 'mf' ? 'MF' : 'FFprobe'
            logDebug(
              `⏱ (Tracks) Audio(${engine}): ${Number(tt.audio_ffprobe_ms || 0).toFixed(1)}ms | Total: ${Number(tt.total_ms || 0).toFixed(1)}ms | Cache: ${tt.cache_hit ? 'hit' : 'miss'}`,
              'info'
            )
            if (Array.isArray(tracks?.debug)) {
              for (const d of tracks.debug) {
                if (!d || typeof d !== 'object') continue
                logDebug(
                  `diag(${String(d.phase || 'tracks')}): program=${d.program_exists ? 'exists' : 'missing'} | exit=${d.exit_code ?? '?'} | success=${Boolean(d.success)} | out=${d.stdout_len}B | err=${d.stderr_len}B`,
                  'info'
                )
                if (d.stderr_head) logDebug(`diag(${String(d.phase || 'tracks')}) stderr: ${String(d.stderr_head).replace(/\\s+/g, ' ').trim()}`, 'info')
              }
            }
          }

          logUser(`Loaded ${streams.length} audio track(s).`, 'success')
          await fetchSubsAsync(path, openSeq)
        } catch (e2) {
          logUser('Audio track loading failed.', 'error')
          logDebug(`Tracks probe failed: ${String(e2?.message || e2)}`, 'error')
        } finally {
          setIsLoadingTracks(false)
        }
      })()

      return

      /*
      // Display performance timing
      if (result.timing_ms) {
        const t = result.timing_ms
        const firstOut = typeof t.ffprobe_first_stdout_byte_ms === 'number' ? t.ffprobe_first_stdout_byte_ms : null
        const firstErr = typeof t.ffprobe_first_stderr_byte_ms === 'number' ? t.ffprobe_first_stderr_byte_ms : null
        const spawnMs = typeof t.ffprobe_spawn_ms === 'number' ? t.ffprobe_spawn_ms : null
        const waitMs = typeof t.ffprobe_wait_ms === 'number' ? t.ffprobe_wait_ms : null
        const cacheHit = Boolean(t.cache_hit)

        const extra =
          firstOut != null || firstErr != null || spawnMs != null || waitMs != null || cacheHit
            ? ` | Spawn: ${spawnMs != null ? spawnMs.toFixed(1) : '-'}ms | 1st out: ${firstOut != null ? firstOut.toFixed(1) : '-'}ms | 1st err: ${firstErr != null ? firstErr.toFixed(1) : '-'}ms | Wait: ${waitMs != null ? waitMs.toFixed(1) : '-'}ms | Cache: ${cacheHit ? 'hit' : 'miss'}`
            : ''

        pushStatus(
          `⏱ Validation: ${t.validation_ms.toFixed(1)}ms | Resolve: ${t.resolve_binaries_ms.toFixed(1)}ms | FFprobe: ${t.ffprobe_execution_ms.toFixed(1)}ms | Parse: ${t.json_parsing_ms.toFixed(1)}ms | Total: ${t.total_ms.toFixed(1)}ms${extra}`,
          'info'
        )
      }

      if (result.ffprobe_path || result.cwd || result.ffprobe_runner) {
        const ffprobePath = result.ffprobe_path ? String(result.ffprobe_path) : ''
        const cwd = result.cwd ? String(result.cwd) : ''
        const runner = result.ffprobe_runner ? String(result.ffprobe_runner) : ''
        pushStatus(`ffprobe: ${ffprobePath || '(unknown)'} | runner: ${runner || '(unknown)'} | cwd: ${cwd || '(unknown)'}`, 'info')
      }

      if (result.ffprobe_args && Array.isArray(result.ffprobe_args)) {
        const t = result.timing_ms
        const ms = t && typeof t.ffprobe_execution_ms === 'number' ? t.ffprobe_execution_ms : null
        if (ms != null && ms > 500) {
          if (result.input_path) {
            pushStatus(`input: ${String(result.input_path)}`, 'info')
          }
          const args = result.ffprobe_args.map((a) => (/\s/.test(String(a)) ? `"${String(a)}"` : String(a))).join(' ')
          pushStatus(`ffprobe cmd: "${String(result.ffprobe_path || 'ffprobe')}" ${args}`, 'info')
        }
      }

      const duration = typeof result.duration_seconds === 'number' ? result.duration_seconds : null
      const allAudio = Array.isArray(result.audio_streams) ? result.audio_streams : []
      const allSubs = Array.isArray(result.subtitle_streams) ? result.subtitle_streams : []
      const streams = allAudio
      const subs = filterEnglishSpanish(allSubs)
      const usedBin = typeof result.ffmpeg_bin_dir_used === 'string' ? result.ffmpeg_bin_dir_used : ''
      setDurationSeconds(duration)
      setAudioStreams(streams)
      setSubtitleStreams(subs)
      if (!ffmpegBinDir && usedBin) setFfmpegBinDir(usedBin)

      // Auto-select first audio track
      const audioPick = pickPreferredStreamIndex(streams)
      setSelectedAudioIndex(audioPick >= 0 ? audioPick : -1)

      if (!touchedOut) {
        if (typeof duration === 'number' && Number.isFinite(duration) && duration > 0) {
          // Set OUT to full video duration by default
          setOutTime(secondsToTime(Math.floor(duration)))
        } else {
          setOutTime('00:00:10')
        }
      }

      // Always pick an audio track when opening a new video (defaulting to first/English),
      // so "No Audio 🔇" is never selected unless there are no streams.
      const audioPick = pickPreferredStreamIndex(streams)
      setSelectedAudioIndex(audioPick >= 0 ? audioPick : -1)

      if (!touchedSubs) {
        const pick = pickPreferredStreamIndex(subs)
        if (pick >= 0) setSelectedSubtitleIndex(pick)
      }

      pushStatus(`Loaded ${streams.length} audio track(s).`, 'success')
      */
    } catch (e) {
      const message = typeof e === 'string' ? e : e?.message ? String(e.message) : String(e)
      if (message.toLowerCase().includes('ffprobe') && message.toLowerCase().includes('not found')) {
        setFfmpegOk(false)
        setFfmpegCheckMessage(message)
      }
      logUser('Error opening file.', 'error')
      logDebug(`Error: ${message}`, 'error')
    } finally {
      setBusyAction('idle')
    }
  }

  async function handleCut() {
    if (!inputPath) {
      logUser('Error: Please open a video file first.', 'error')
      return
    }
    if (!ffmpegOk) {
      setFfmpegRequiredModalOpen(true)
      logUser('FFmpeg is required to cut/export clips.', 'error')
      return
    }

    if (!inParsed.ok) return
    if (!outParsed.ok) return
    if (outParsed.seconds <= inParsed.seconds) return

    if (mode !== 'lossless' && mode !== 'exact') {
      logUser("Error: Mode must be 'Lossless' or 'Exact'.", 'error')
      return
    }

    if (mode === 'lossless') {
      const currentKey = losslessPreflightKey(inputPath, inTime, outTime, ffmpegBinDir)
      if (skipLosslessPreflightKeyRef.current === currentKey) {
        skipLosslessPreflightKeyRef.current = ''
      } else {
        const requestId = ++losslessPreflightReqRef.current

        // Always show the modal immediately to avoid flicker
        setLosslessModal({
          open: true,
          checking: true,
          inTime,
          outTime,
          shiftSeconds: null,
          keyframeTime: null,
          keyframeSeconds: null,
          nextKeyframeTime: null,
          nextKeyframeDelta: null,
          nextKeyframeSeconds: null,
          message: '',
        })

        try {
          const pre = await invoke('lossless_preflight', { inputPath, inTime, ffmpegBinDir })
          if (losslessPreflightReqRef.current !== requestId) return

          const shift = typeof pre?.start_shift_seconds === 'number' ? pre.start_shift_seconds : null
          const keyframe = typeof pre?.nearest_keyframe_seconds === 'number' ? pre.nearest_keyframe_seconds : null
          const nextKeyframe = typeof pre?.next_keyframe_seconds === 'number' ? pre.next_keyframe_seconds : null

          if (shift != null && shift > 0.0005 && keyframe != null) {
            const keyframeLabel = secondsToTimeRounded(keyframe)
            const nextLabel = nextKeyframe != null ? secondsToTimeRounded(nextKeyframe) : null
            const nextDelta =
              nextKeyframe != null && inParsed.ok && nextKeyframe > inParsed.seconds
                ? formatDuration(nextKeyframe - inParsed.seconds)
                : null

            // Transition to report view
            setLosslessModal({
              open: true,
              checking: false,
              inTime,
              outTime,
              shiftSeconds: shift,
              keyframeTime: keyframeLabel,
              keyframeSeconds: keyframe,
              nextKeyframeTime: nextLabel,
              nextKeyframeDelta: nextDelta,
              nextKeyframeSeconds: nextKeyframe,
              message: '',
            })
            return
          }

          // No issue, close modal and proceed
          setLosslessModal({
            open: false,
            checking: false,
            message: '',
            inTime: '',
            outTime: '',
            shiftSeconds: null,
            keyframeTime: null,
            keyframeSeconds: null,
            nextKeyframeTime: null,
            nextKeyframeDelta: null,
            nextKeyframeSeconds: null,
          })
        } catch (e) {
          if (losslessPreflightReqRef.current !== requestId) return
          setLosslessModal({
            open: true,
            checking: false,
            inTime,
            outTime,
            shiftSeconds: null,
            keyframeTime: null,
            keyframeSeconds: null,
            nextKeyframeTime: null,
            nextKeyframeDelta: null,
            nextKeyframeSeconds: null,
            message: 'Lossless can only cut on keyframes, so the clip may start earlier than IN. Use Exact for frame-accurate start.',
          })
          logDebug(`Lossless preflight failed: ${String(e?.message || e)}`, 'error')
          return
        }
      }
    }

    setBusyAction('cutting')
    logUser('Cutting…', 'info')
    setOutputPath('')
    try {
      const result = await invoke('trim_media', {
        inputPath,
        inTime,
        outTime,
        mode,
        audioStreamIndex: selectedAudioIndex,
        subtitleStreamIndex: selectedSubtitleIndex,
        ffmpegBinDir,
      })

      if (!result?.output_path) {
        logUser('Error: Trim completed but no output path was returned.', 'error')
        return
      }

      setOutputPath(result.output_path)
      logUser('Cut finished.', 'success')
      logDebug(`Done. Output: ${result.output_path}`, 'success')
    } catch (e) {
      const message = typeof e === 'string' ? e : e?.message ? String(e.message) : String(e)
      logUser('Cut failed.', 'error')
      logDebug(`Error: ${message}`, 'error')
    } finally {
      setBusyAction('idle')
    }
  }

  async function proceedLosslessAfterModal() {
    skipLosslessPreflightKeyRef.current = losslessPreflightKey(inputPath, inTime, outTime, ffmpegBinDir)
    setLosslessModal({
      open: false,
      checking: false,
      message: '',
      inTime: '',
      outTime: '',
      shiftSeconds: null,
      keyframeTime: null,
      keyframeSeconds: null,
      nextKeyframeTime: null,
      nextKeyframeDelta: null,
      nextKeyframeSeconds: null,
    })
    requestAnimationFrame(() => handleCut())
  }

  function switchToExactFromModal() {
    setLosslessModal({
      open: false,
      checking: false,
      message: '',
      inTime: '',
      outTime: '',
      shiftSeconds: null,
      keyframeTime: null,
      keyframeSeconds: null,
      nextKeyframeTime: null,
      nextKeyframeDelta: null,
      nextKeyframeSeconds: null,
    })
    setMode('exact')
    logUser('Switched to Exact mode.', 'info')
  }

  function closeLosslessModal() {
    losslessPreflightReqRef.current += 1
    setLosslessModal({
      open: false,
      checking: false,
      message: '',
      inTime: '',
      outTime: '',
      shiftSeconds: null,
      keyframeTime: null,
      keyframeSeconds: null,
      nextKeyframeTime: null,
      nextKeyframeDelta: null,
      nextKeyframeSeconds: null,
    })
    setLosslessTipExpanded(false)
  }

  function useNextKeyframe() {
    if (losslessModal.nextKeyframeSeconds == null) return

    // The user selected this keyframe from the "next keyframe" suggestion
    // We already know from the previous check that this IS a valid keyframe
    const exactSeconds = losslessModal.nextKeyframeSeconds
    const milliseconds = Math.round((exactSeconds - Math.floor(exactSeconds)) * 1000)

    // Send the EXACT time with milliseconds to the backend
    // Backend now accepts hh:mm:ss.milliseconds format (e.g., 00:00:03.170)
    // This ensures FFmpeg cuts at the exact keyframe position
    const timeForBackend = formatSecondsToHhMmSsWithMillis(exactSeconds)
    const displayTime = milliseconds > 0 ? `${secondsToTime(Math.floor(exactSeconds))} (${milliseconds}ms)` : secondsToTime(Math.floor(exactSeconds))

    setInTime(timeForBackend)
    setTouchedIn(true)

    logDebug(`[Keyframe] User selected keyframe at ${exactSeconds}s → setting IN to ${timeForBackend}`, 'success')

    setLosslessModal({
      open: true,
      checking: false,
      inTime: displayTime,
      outTime: losslessModal.outTime,
      shiftSeconds: 0,  // Zero shift - perfect alignment
      keyframeTime: displayTime,
      keyframeSeconds: exactSeconds,
      nextKeyframeTime: null,
      nextKeyframeDelta: null,
      nextKeyframeSeconds: null,
      message: '',
    })

    logUser(`IN time set to ${timeForBackend} (keyframe at ${displayTime})`, 'success')
  }

  async function handleRevealOutput() {
    if (!outputPath) return
    try {
      const opener = await import('@tauri-apps/plugin-opener')
      if (typeof opener.revealItemInDir === 'function') {
        await opener.revealItemInDir(outputPath)
        return
      }
    } catch (_) {}

    try {
      await invoke('plugin:opener|reveal_item_in_dir', { path: outputPath })
      return
    } catch (_) {}

    await invoke('plugin:opener|reveal_item_in_dir', { paths: [outputPath] })
  }

  async function handleCopyOutputPath() {
    if (!outputPath) return
    try {
      await navigator.clipboard.writeText(outputPath)
      logUser('Copied output path to clipboard.', 'success')
    } catch (e) {
      const message = typeof e === 'string' ? e : e?.message ? String(e.message) : String(e)
      logUser(`Error: Failed to copy: ${message}`, 'error')
    }
  }

  const handleTimeIncrement = useCallback((isIn, amount) => {
    if (isIn) {
      setTouchedIn(true)
      setInTime((prevTime) => {
        const parsed = parseHhMmSs(prevTime)
        if (!parsed.ok) return prevTime
        const newSeconds = Math.max(0, Math.min(359999, parsed.seconds + amount))
        const hh = Math.floor(newSeconds / 3600)
        const mm = Math.floor((newSeconds % 3600) / 60)
        const ss = newSeconds % 60
        return `${String(hh).padStart(2, '0')}:${String(mm).padStart(2, '0')}:${String(ss).padStart(2, '0')}`
      })
    } else {
      setTouchedOut(true)
      setOutTime((prevTime) => {
        const parsed = parseHhMmSs(prevTime)
        if (!parsed.ok) return prevTime
        const newSeconds = Math.max(0, Math.min(359999, parsed.seconds + amount))
        const hh = Math.floor(newSeconds / 3600)
        const mm = Math.floor((newSeconds % 3600) / 60)
        const ss = newSeconds % 60
        return `${String(hh).padStart(2, '0')}:${String(mm).padStart(2, '0')}:${String(ss).padStart(2, '0')}`
      })
    }
  }, [])

  const handleArrowMouseDown = useCallback((isIn, amount) => {
    return (e) => {
      e.preventDefault()
      isAdjustingTimeRef.current = true
      if (holdTimeoutRef.current) clearTimeout(holdTimeoutRef.current)
      if (holdIntervalRef.current) clearInterval(holdIntervalRef.current)

      handleTimeIncrement(isIn, amount)

      holdTimeoutRef.current = setTimeout(() => {
        holdIntervalRef.current = setInterval(() => {
          handleTimeIncrement(isIn, amount)
        }, 100)
      }, 500)
    }
  }, [handleTimeIncrement])

  const handleArrowMouseUp = useCallback(() => {
    if (holdTimeoutRef.current) {
      clearTimeout(holdTimeoutRef.current)
      holdTimeoutRef.current = null
    }
    if (holdIntervalRef.current) {
      clearInterval(holdIntervalRef.current)
      holdIntervalRef.current = null
    }
    if (isAdjustingTimeRef.current) {
      isAdjustingTimeRef.current = false
      setTimeAdjustToken((n) => n + 1)
    }
  }, [])

  const canReveal = Boolean(outputPath)
  const audioCountText = inputPath ? `${audioStreams.length} audio track(s)` : '-'
  const subtitleCountText = inputPath
    ? isLoadingSubs
      ? 'loading.'
      : subsStatus === 'loaded_none'
        ? 'none'
        : `${subtitleStreams.length} subtitle track(s)`
    : '-'

  return (
    <div className="vt-root">

      <div className="vt-shell">
        {losslessModal.open ? (
          <div
            className="vt-modalOverlay"
            role="dialog"
            aria-modal="true"
            aria-label="Lossless cut keyframe check"
            onMouseDown={(e) => {
              if (e.target === e.currentTarget && !losslessModal.checking) {
                closeLosslessModal()
              }
            }}
          >
            <div className="vt-modal vt-losslessModal">
              {losslessModal.checking ? (
                <>
                  <div className="vt-modalHeader">
                    <p className="vt-modalTitle">Checking Keyframes</p>
                  </div>
                  <div className="vt-modalBody vt-losslessChecking">
                    <div className="vt-spinner" aria-hidden="true" />
                    <p>Analyzing video keyframes...</p>
                  </div>
                  <div className="vt-modalActions">
                    <button type="button" className="vt-button vt-buttonCancel" onClick={closeLosslessModal}>
                      Cancel
                    </button>
                  </div>
                </>
              ) : losslessModal.message ? (
                <>
                  <div className="vt-modalHeader">
                    <p className="vt-modalTitle">⚠ Lossless Limitation</p>
                  </div>
                  <div className="vt-modalBody">
                    <p style={{ marginBottom: 'var(--space-3)', lineHeight: '1.5' }}>{losslessModal.message}</p>
                  </div>
                  <div className="vt-modalActions">
                    <button type="button" className="vt-button vt-buttonCancel" onClick={closeLosslessModal}>
                      Cancel
                    </button>
                    <button type="button" className="vt-button" onClick={switchToExactFromModal}>
                      Use Exact
                    </button>
                    <button type="button" className="vt-button vt-buttonPrimary" onClick={proceedLosslessAfterModal}>
                      Continue Anyway
                    </button>
                  </div>
                </>
              ) : (
                <>
                  <div className="vt-modalHeader">
                    <p className="vt-modalTitle">
                      {losslessModal.shiftSeconds != null && losslessModal.shiftSeconds > 0.0005
                        ? '⚠ Keyframe Alignment Issue'
                        : '✓ Keyframe Alignment Success'}
                    </p>
                  </div>
                  <div className={`vt-modalBody vt-losslessReport ${losslessModal.shiftSeconds != null && losslessModal.shiftSeconds > 0.0005 ? '' : 'vt-losslessReportSuccess'}`}>
                    <div className={`vt-losslessWarning ${losslessModal.shiftSeconds != null && losslessModal.shiftSeconds > 0.0005 ? '' : 'vt-losslessWarningSuccess'}`}>
                      <p><strong>Lossless mode can only cut at keyframe positions.</strong></p>
                      {losslessModal.shiftSeconds != null && losslessModal.shiftSeconds > 0.0005 ? (
                        <p>Your requested IN time doesn't align with a keyframe, so the clip will start earlier.</p>
                      ) : (
                        <p className="vt-losslessSuccess">✓ Your requested IN time aligns perfectly with a keyframe!</p>
                      )}
                    </div>

                    <div className="vt-losslessComparison">
                      <div className="vt-losslessComparisonRow">
                        <div className="vt-losslessLabel">Your Request</div>
                        <div className="vt-losslessTimeRange">
                          <span className="vt-losslessTime">{losslessModal.inTime}</span>
                          <span className="vt-losslessArrow">→</span>
                          <span className="vt-losslessTime">{losslessModal.outTime}</span>
                        </div>
                      </div>

                      <div className="vt-losslessDivider" />

                      <div className="vt-losslessComparisonRow">
                        <div className="vt-losslessLabel">Actual Result</div>
                        <div className="vt-losslessTimeRange">
                          <span className="vt-losslessTime vt-losslessHighlight">
                            {losslessModal.keyframeTime}
                          </span>
                          <span className="vt-losslessArrow">→</span>
                          <span className="vt-losslessTime">{losslessModal.outTime}</span>
                        </div>
                      </div>

                      {losslessModal.shiftSeconds != null && (
                        <div className="vt-losslessImpact">
                          <span className="vt-losslessImpactLabel">Extra footage at start:</span>
                          <span className="vt-losslessImpactValue">{formatDuration(losslessModal.shiftSeconds)}</span>
                        </div>
                      )}
                    </div>

                    {losslessModal.nextKeyframeTime && (
                      <div className="vt-losslessAlternative">
                        <p className="vt-losslessAlternativeTitle">Alternative: Next keyframe</p>
                        <div className="vt-losslessAlternativeTime">
                          <div className="vt-losslessAlternativeTimeInfo">
                            <span>{losslessModal.nextKeyframeTime}</span>
                            {losslessModal.nextKeyframeDelta && (
                              <span className="vt-losslessAlternativeDelta">(+{losslessModal.nextKeyframeDelta})</span>
                            )}
                          </div>
                          <div className="vt-losslessAlternativeButton">
                            <button type="button" className="vt-button vt-buttonPrimary" onClick={useNextKeyframe}>
                              Use This Keyframe
                            </button>
                          </div>
                        </div>
                      </div>
                    )}

                    <div className="vt-losslessTip">
                      <p style={{ cursor: 'pointer', userSelect: 'none' }} onClick={() => setLosslessTipExpanded(!losslessTipExpanded)}>
                        <strong>Why does this happen?</strong> {losslessTipExpanded ? '▼' : '▶'}
                      </p>
                      {losslessTipExpanded && (
                        <>
                          <p>Video files have keyframes (full frames) and delta frames (changes). Lossless mode copies frames without re-encoding, so it can only start at keyframes. Keyframe spacing varies by codec and encoding settings.</p>
                          <p><strong>Solution:</strong> Use Exact mode for frame-accurate cuts (requires re-encoding).</p>
                        </>
                      )}
                    </div>
                  </div>
                  <div className="vt-modalActions">
                    <button type="button" className="vt-button vt-buttonCancel" onClick={closeLosslessModal}>
                      Cancel
                    </button>
                    <button type="button" className="vt-button" onClick={switchToExactFromModal}>
                      Switch to Exact
                    </button>
                    <button type="button" className="vt-button vt-buttonPrimary" onClick={proceedLosslessAfterModal}>
                      Continue Lossless
                    </button>
                  </div>
                </>
              )}
            </div>
          </div>
        ) : null}
        {ffmpegRequiredModalOpen && !ffmpegOkCache ? (
          <div
            className="vt-modalOverlay"
            role="dialog"
            aria-modal="true"
            aria-label="FFmpeg required"
          >
            <div className="vt-modal">
              <div className="vt-modalHeader">
                <p className="vt-modalTitle">FFmpeg required</p>
              </div>
              <div className="vt-modalBody">
                Clip Wave cannot cut or export clips without FFmpeg. Install it once (~120MB) and you’re ready to go.
              </div>
              <div className="vt-modalActions">
                <button
                  type="button"
                  className="vt-button"
                  onClick={async () => {
                    try {
                      await runCheckFfmpeg()
                      if (!ffmpegOk) logUser('FFmpeg not detected yet. Please install it.', 'error')
                    } catch {
                      // ignore
                    }
                  }}
                  disabled={busy || isDownloadingFfmpeg}
                >
                  I already installed it
                </button>
                <button
                  type="button"
                  className="vt-button vt-buttonPrimary"
                  onClick={async () => {
                    setFfmpegRequiredModalOpen(false)
                    await handleDownloadFfmpegDirect()
                  }}
                  disabled={busy || isDownloadingFfmpeg}
                >
                  Install FFmpeg
                </button>
              </div>
            </div>
          </div>
        ) : null}
        <div className="vt-card">
          <div className="vt-header">
            <div className="vt-titleRow">
              <img src={clipwaveLogo} alt="Clip Wave" className="vt-logo" />
              <h1 className="vt-title">Clip Wave</h1>
            </div>
            <p className="vt-subtitle">
              <span
                onClick={async (e) => {
                  e.stopPropagation()
                  logUser('Opening FFmpeg website...', 'info')
                  logDebug('FFmpeg link clicked', 'info')
                  try {
                    await openUrl('https://ffmpeg.org')
                    logDebug('openUrl called successfully', 'success')
                  } catch (err) {
                    console.error('Failed to open FFmpeg URL:', err)
                    logUser(`Failed to open FFmpeg website: ${String(err?.message || err)}`, 'error')
                    logDebug(`openUrl error: ${String(err?.message || err)}`, 'error')
                  }
                }}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault()
                    logUser('Opening FFmpeg website (keyboard)...', 'info')
                    openUrl('https://ffmpeg.org').catch(err => {
                      logUser(`Failed to open FFmpeg website: ${String(err?.message || err)}`, 'error')
                    })
                  }
                }}
                style={{
                  textDecoration: 'underline',
                  cursor: 'pointer',
                  display: 'inline-block',
                  userSelect: 'none'
                }}
                title="FFmpeg is licensed under LGPL 2.1+ - Click to learn more"
              >
                FFmpeg
              </span>
              {' '}cut tool (v0.1)
            </p>
          </div>

          {!ffmpegOk && !ffmpegOkCache && (
            <div className="vt-ffmpegPanel">
              <div className="vt-ffmpegTitleRow">
                <p className="vt-ffmpegTitle">FFmpeg required</p>
                <p className="vt-sectionMeta">&nbsp;</p>
              </div>
              <p className="vt-ffmpegBody">
                Clip Wave cannot cut or export clips without FFmpeg. Install it once (~120MB) and Clip Wave will use it automatically.
              </p>
              <p className="vt-ffmpegBody" style={{ fontSize: 'var(--text-xs)', fontStyle: 'italic' }}>
                FFmpeg is free software (LGPL 2.1+) -{' '}
                <span
                  onClick={() => openUrl('https://ffmpeg.org')}
                  style={{ textDecoration: 'underline', cursor: 'pointer' }}
                >
                  Learn more
                </span>
              </p>
              <p className="vt-ffmpegBody" style={{ fontSize: 'var(--text-xs)', fontStyle: 'italic', opacity: 0.7 }}>
                Windows builds by{' '}
                <span
                  onClick={() => openUrl('https://www.gyan.dev/ffmpeg/builds/')}
                  style={{ textDecoration: 'underline', cursor: 'pointer' }}
                >
                  Gyan Doshi
                </span>
              </p>
              <p className="vt-ffmpegBody" style={{ fontSize: 'var(--text-xs)', fontStyle: 'italic' }}>
                If you already installed FFmpeg yourself, make sure <span style={{ fontFamily: 'var(--mono)' }}>ffmpeg</span> and{' '}
                <span style={{ fontFamily: 'var(--mono)' }}>ffprobe</span> are on your PATH, then click Check again.
              </p>
              <div className="vt-ffmpegActions">
                <button
                  type="button"
                  className="vt-button vt-buttonPrimary"
                  onClick={handleDownloadFfmpegDirect}
                  disabled={busy || isDownloadingFfmpeg}
                >
                  {isDownloadingFfmpeg ? 'Installing…' : 'Install FFmpeg (~120MB)'}
                </button>
                <button type="button" className="vt-button" onClick={() => runCheckFfmpeg()} disabled={busy || isDownloadingFfmpeg || isCheckingFfmpeg}>
                  Check again
                </button>
                {debugLogsEnabled ? (
                  <button
                    type="button"
                    className="vt-button"
                    onClick={handleAddDefenderExclusionForFfmpeg}
                    disabled={busy || isDownloadingFfmpeg}
                  >
                    Add Defender exclusion.
                  </button>
                ) : null}
              </div>
              {false && isDownloadingFfmpeg ? (
                <div className="vt-progressWrap">
                  <div className={`vt-progressTrack ${ffmpegInstallProgress?.progress == null ? 'vt-progressIndeterminate' : ''}`}>
                    {ffmpegInstallProgress?.progress != null ? (
                      <div className="vt-progressFill" style={{ width: `${Math.round(ffmpegInstallProgress.progress * 100)}%` }} />
                    ) : (
                      <div className="vt-progressFill" style={{ width: '35%' }} />
                    )}
                  </div>
                  <div className="vt-progressMeta">
                    <span>
                      {ffmpegInstallOverlayText || 'Installing…'}
                    </span>
                    <span>
                      {ffmpegInstallProgress?.progress != null
                        ? `${Math.round(ffmpegInstallProgress.progress * 100)}%`
                        : ffmpegInstallProgress?.bytesDone != null && ffmpegInstallProgress?.bytesTotal != null
                          ? `${formatBytes(ffmpegInstallProgress.bytesDone)} / ${formatBytes(ffmpegInstallProgress.bytesTotal)}`
                          : ''}
                    </span>
                  </div>
                </div>
              ) : null}
            </div>
          )}

          {/* Old WinGet-based FFmpeg panel disabled; keep "Fast FFmpeg" panel only. */}
          {false && !ffmpegOk && (
            <div className="vt-ffmpegPanel">
              <div className="vt-ffmpegTitleRow">
                <p className="vt-ffmpegTitle">FFmpeg missing</p>
                <p className="vt-sectionMeta">{isCheckingFfmpeg || isCheckingWinget ? 'Checking…' : ''}</p>
              </div>
              <p className="vt-ffmpegBody">
                This app requires <span style={{ fontFamily: 'var(--mono)' }}>ffmpeg</span> and{' '}
                <span style={{ fontFamily: 'var(--mono)' }}>ffprobe</span>. Recommended install (Windows 11):
              </p>
              <div className="vt-ffmpegCmd" title={wingetCommand}>
                {wingetCommand}
              </div>
              {ffmpegCheckMessage ? <p className="vt-ffmpegBody">FFmpeg check: {ffmpegCheckMessage}</p> : null}
              {!wingetAvailable && wingetMessage ? <p className="vt-ffmpegBody">WinGet: {wingetMessage}</p> : null}
              {wingetAvailable && (
                <p className="vt-ffmpegBody" style={{ fontSize: 'var(--text-xs)', fontStyle: 'italic' }}>
                  After installation completes, click Re-check to detect FFmpeg.
                </p>
              )}
              <div className="vt-ffmpegActions">
                {wingetAvailable ? (
                  <button type="button" className="vt-button vt-buttonPrimary" onClick={handleInstallFfmpeg} disabled={busy}>
                    Install (WinGet)…
                  </button>
                ) : (
                  <button
                    type="button"
                    className="vt-button vt-buttonPrimary"
                    onClick={() => openUrl('ms-settings:appsfeatures-appaliases')}
                    disabled={busy}
                  >
                    Open app execution aliases…
                  </button>
                )}
                <button
                  type="button"
                  className="vt-button"
                  onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(wingetCommand)
                      pushStatus('Copied WinGet command.', 'success')
                    } catch {
                      pushStatus('Failed to copy command.', 'error')
                    }
                  }}
                  disabled={busy}
                >
                  Copy command
                </button>
                <button type="button" className="vt-button" onClick={() => runCheckFfmpeg()} disabled={busy || isCheckingFfmpeg || isCheckingWinget}>
                  Re-check
                </button>
                <button type="button" className="vt-button" onClick={() => openUrl('https://www.gyan.dev/ffmpeg/builds/')} disabled={busy}>
                  Download page…
                </button>
              </div>
              <p className="vt-ffmpegBody">If you already have FFmpeg, make sure both executables are available on your PATH.</p>
            </div>
          )}

          {isDownloadingFfmpeg ? (
            <div className="vt-blockingOverlay" role="status" aria-live="polite">
              <div className="vt-blockingModal">
                <div className="vt-spinner" aria-hidden="true" />
                <div>
                  <div className="vt-blockingTitle">Installing FFmpeg.</div>
                  <div className="vt-blockingBody">
                    {ffmpegInstallOverlayText || 'Downloading and installing. This can take a minute.'}
                  </div>
                </div>
                <div className="vt-blockingProgressFull">
                  <div className={`vt-progressTrack ${ffmpegInstallProgress?.progress == null ? 'vt-progressIndeterminate' : ''}`}>
                    {ffmpegInstallProgress?.progress != null ? (
                      <div className="vt-progressFill" style={{ width: `${Math.round(ffmpegInstallProgress.progress * 100)}%` }} />
                    ) : (
                      <div className="vt-progressFill" style={{ width: '35%' }} />
                    )}
                  </div>
                  <div className="vt-progressMeta">
                    <span>
                      {ffmpegInstallProgress?.phase ? String(ffmpegInstallProgress.phase) : ''}
                    </span>
                    <span>
                      {ffmpegInstallProgress?.progress != null
                        ? `${Math.round(ffmpegInstallProgress.progress * 100)}%`
                        : ffmpegInstallProgress?.bytesDone != null && ffmpegInstallProgress?.bytesTotal != null
                          ? `${formatBytes(ffmpegInstallProgress.bytesDone)} / ${formatBytes(ffmpegInstallProgress.bytesTotal)}`
                          : ''}
                    </span>
                  </div>
                </div>
              </div>
            </div>
          ) : null}

          <div className="vt-sections">
            <div
              className="vt-section vt-sectionSticky"
            >
              <div className="vt-sectionHeader">
                <p className="vt-sectionTitle">Source</p>
                <p className="vt-sectionMeta">&nbsp;</p>
              </div>

              <div className="vt-grid" style={{ marginBottom: 'var(--space-3)' }}>
                <div className="vt-label"></div>
                <div className="vt-chipRow">
                  <span className="vt-chip"><b>⏱ Duration</b> {durationText || '-'}</span>
                  <span className="vt-chip"><b>🔊 Audio</b> {audioCountText}</span>
                  <span className="vt-chip"><b>📄 Subs</b> {subtitleCountText}</span>
                </div>
              </div>

              <div className="vt-grid">
                <div className="vt-label">Video file</div>
                <div className="vt-controlRow">
                  <div className="vt-videoRow">
                    <button
                      type="button"
                      className="vt-button vt-buttonPrimary"
                      onClick={handleOpenVideo}
                      disabled={busy}
                    >
                      Open video…
                    </button>
                    <div className="vt-readonly vt-path vt-scrollX" title={inputPath || ''}>
                      {inputPath || 'No file selected'}
                    </div>
                  </div>
                </div>

              </div>
            </div>

            <div className="vt-section">
              <div className="vt-sectionHeader">
                <p className="vt-sectionTitle">Cut</p>
                <p className="vt-sectionMeta">&nbsp;</p>
              </div>

              <div className="vt-sectionContent">
                <div className="vt-timeRow">
                  <div className="vt-label">Trim</div>
                  <div className="vt-timeInputsWrapper">
                    <div className="vt-timeInputs">
                      <div className="vt-timeLabel">
                        <span>IN</span>
                      </div>
                      <div className="vt-timeInputGroup">
                        <TimeInput
                          value={inTime}
                          onChange={(v) => {
                            setTouchedIn(true)
                            setInTime(v)
                          }}
                          disabled={busy}
                          ariaLabel="IN time"
                          className={inError ? 'vt-invalid' : ''}
                          maxSeconds={durationSeconds != null ? Math.floor(durationSeconds) : undefined}
                        />
                        <div className="vt-timeArrows">
                          <button
                            type="button"
                            className="vt-timeArrow"
                            onMouseDown={handleArrowMouseDown(true, 1)}
                            onMouseUp={handleArrowMouseUp}
                            onMouseLeave={handleArrowMouseUp}
                            disabled={busy}
                            title="Add 1 second (hold to repeat)"
                          >
                            ▲
                          </button>
                          <button
                            type="button"
                            className="vt-timeArrow"
                            onMouseDown={handleArrowMouseDown(true, -1)}
                            onMouseUp={handleArrowMouseUp}
                            onMouseLeave={handleArrowMouseUp}
                            disabled={busy}
                            title="Subtract 1 second (hold to repeat)"
                          >
                            ▼
                          </button>
                        </div>
                      </div>
                      <div className="vt-timeLabel">
                        <span>OUT</span>
                      </div>
                      <div className="vt-timeInputGroup">
                        <TimeInput
                          value={outTime}
                          onChange={(v) => {
                            setTouchedOut(true)
                            setOutTime(v)
                          }}
                          disabled={busy}
                          ariaLabel="OUT time"
                          className={outError || rangeError ? 'vt-invalid' : ''}
                          maxSeconds={durationSeconds != null ? Math.floor(durationSeconds) : undefined}
                        />
                        <div className="vt-timeArrows">
                          <button
                            type="button"
                            className="vt-timeArrow"
                            onMouseDown={handleArrowMouseDown(false, 1)}
                            onMouseUp={handleArrowMouseUp}
                            onMouseLeave={handleArrowMouseUp}
                            disabled={busy}
                            title="Add 1 second (hold to repeat)"
                          >
                            ▲
                          </button>
                          <button
                            type="button"
                            className="vt-timeArrow"
                            onMouseDown={handleArrowMouseDown(false, -1)}
                            onMouseUp={handleArrowMouseUp}
                            onMouseLeave={handleArrowMouseUp}
                            disabled={busy}
                            title="Subtract 1 second (hold to repeat)"
                          >
                            ▼
                          </button>
                        </div>
                      </div>
                      <button
                        type="button"
                        className="vt-resetBtn"
                        onClick={() => {
                          setTouchedIn(true)
                          setTouchedOut(true)
                          setInTime('00:00:00')
                          // Reset OUT to full video duration if known, otherwise 10 seconds
                          const outDefault = typeof durationSeconds === 'number' && durationSeconds > 0
                            ? secondsToTime(Math.floor(durationSeconds))
                            : '00:00:10'
                          setOutTime(outDefault)
                        }}
                        disabled={busy}
                        title="Reset to full video range"
                      >
                        Reset
                      </button>
                    </div>
                    {(inError || outError || rangeError) && (
                      <div className="vt-errorContainer">
                        {inError ? <div className="vt-inlineError">IN: {inError}</div> : null}
                        {outError ? <div className="vt-inlineError">OUT: {outError}</div> : null}
                        {!outError && rangeError ? <div className="vt-inlineError">{rangeError}</div> : null}
                      </div>
                    )}
                  </div>
                </div>
              </div>

              <div className="vt-grid">

                <div className="vt-label">Mode</div>
                <div className="vt-controlRow">
                  <div className="vt-segmented" role="group" aria-label="Mode">
                    <button
                      type="button"
                      className={`vt-segBtn ${mode === 'lossless' ? 'vt-segBtnActive' : ''}`}
                      onClick={() => setMode('lossless')}
                      disabled={busy}
                    >
                      Lossless
                    </button>
                    <button
                      type="button"
                      className={`vt-segBtn ${mode === 'exact' ? 'vt-segBtnActive' : ''}`}
                      onClick={() => setMode('exact')}
                      disabled={busy}
                    >
                      Exact
                    </button>
                  </div>
                  <div className="vt-modeHelp">
                    {mode === 'lossless'
                      ? 'Lossless is fastest (stream copy). Clip may start at the nearest keyframe.'
                      : 'Exact re-encodes video for frame-accurate start. Audio/subtitles are copied.'}
                  </div>
                  {false && (
                    <div className="vt-keyframeWarning">
                      ⚠ Lossless starts on keyframes. Your IN is {keyframeInfo.inTime}; the nearest keyframe at/before IN is {keyframeInfo.keyframeTime}.
                      The output will start at {keyframeInfo.keyframeTime} and include {keyframeInfo.extraAtStart} extra before your IN. Use Exact for frame-accurate start.
                    </div>
                  )}
                </div>

                <div className="vt-label vt-labelMiddle">Tracks</div>
                <div className="vt-controlRow">
                  <div className="vt-dualSelectRow">
                    <div className="vt-dualSelectCol">
                      <label className="vt-miniLabel" htmlFor="vt-audioSelect">
                        🔊 Audio (<span style={{ color: audioStreams.length > 0 ? 'var(--success)' : 'inherit' }}>{audioStreams.length}</span>)
                      </label>
                      <div className="vt-selectWrap">
                        <select
                          id="vt-audioSelect"
                          className="vt-select"
                          value={String(selectedAudioIndex)}
                          onChange={(e) => {
                            setTouchedAudio(true)
                            setSelectedAudioIndex(Number(e.target.value))
                          }}
                          disabled={busy || !inputPath}
                        >
                          <option value="-1">No Audio 🔇</option>
                          {audioStreams.map((s) => (
                            <option key={String(s.order ?? s.index)} value={String(s.order ?? s.index)}>
                              {audioLabel(s)}
                            </option>
                          ))}
                        </select>
                      </div>
                    </div>
                    <div className="vt-dualSelectCol">
                      <label className="vt-miniLabel" htmlFor="vt-subtitleSelect">
                        📄 Subtitles (<span style={{ color: subtitleStreams.length > 0 ? 'var(--success)' : 'inherit' }}>{subtitleStreams.length}</span>)
                      </label>
                      <div className="vt-selectWrap">
                        <select
                          id="vt-subtitleSelect"
                          className="vt-select"
                          value={String(selectedSubtitleIndex)}
                          onChange={(e) => {
                            setTouchedSubs(true)
                            setSelectedSubtitleIndex(Number(e.target.value))
                          }}
                          disabled={busy || !inputPath || subsStatus === 'loading'}
                        >
                          {!inputPath ? (
                            <option value="-1">Open a video to load subtitles.</option>
                          ) : subsStatus === 'loading' ? (
                            <option value="-1">Loading subtitles…</option>
                          ) : subsStatus === 'loaded_none' ? (
                            <option value="-1">No subtitles found</option>
                          ) : (
                            <>
                              <option value="-1">No subtitles</option>
                              {subtitleStreams.map((s) => (
                                <option key={String(s.index)} value={String(s.index)}>
                                  {subtitleLabel(s)}
                                </option>
                              ))}
                            </>
                          )}
                        </select>
                      </div>

                    </div>
                  </div>

                </div>

                <div className="vt-label">Actions</div>
                <div className="vt-controlRow">
                  <div className="vt-actions">
                    <button
                      type="button"
                      className="vt-button vt-buttonPrimary"
                      onClick={handleCut}
                      disabled={!canCut}
                    >
                      Cut
                    </button>
                    <button
                      type="button"
                      className="vt-button"
                      onClick={handleRevealOutput}
                      disabled={!canReveal}
                    >
                      Reveal output
                    </button>
                    <button
                      type="button"
                      className="vt-button"
                      onClick={handleCopyOutputPath}
                      disabled={!canReveal}
                    >
                      Copy path
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div className="vt-status">
            <div className="vt-statusHeader">
              <div className="vt-statusTitle">Log</div>
              <div className="vt-statusActions">
                <label className="vt-inlineCheck" style={{ marginRight: 'var(--space-2)' }}>
                  <input
                    type="checkbox"
                    checked={debugLogsEnabled}
                    onChange={(e) => setDebugLogsEnabled(Boolean(e.target.checked))}
                    disabled={busy}
                  />
                  <span>Enable debug logs</span>
                </label>
                <button type="button" className="vt-button" onClick={handleRefreshApp} disabled={busy}>
                  Refresh
                </button>
                <button type="button" className="vt-button" onClick={handleClearLogs} disabled={busy || statusLog.length === 0}>
                  Clear logs
                </button>
              </div>
            </div>

            <div className="vt-statusBody">
              <ul className="vt-log">
                {visibleStatusLog.map((line) => (
                  <li className="vt-logItem" key={String(line.id)}>
                    <span className="vt-logTimestamp">
                      {new Date(line.ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })}
                    </span>
                    <span
                      className={
                        `vt-logDot ${
                          line.kind === 'error'
                            ? 'vt-logDotError'
                            : line.kind === 'success'
                              ? 'vt-logDotSuccess'
                              : 'vt-logDotInfo'
                        }`
                      }
                    />
                    <span className="vt-logText">{line.text}</span>
                  </li>
                ))}
              </ul>
              {visibleStatusLog.length === 0 ? <div className="vt-statusEmpty">No log entries.</div> : null}
            </div>
          </div>

        </div>
      </div>
    </div>
  )
}

export default App
