import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type JSX,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react"
import { invoke } from "@tauri-apps/api/core"

// ── Wire types — match koi_common::ceremony serde ─────────────────

type MessageKind = "info" | "qr_code" | "summary" | "error"

interface Message {
  kind: MessageKind
  title: string
  content: string
}

type InputType =
  | "select_one"
  | "select_many"
  | "text"
  | "secret"
  | "secret_confirm"
  | "code"
  | "entropy"
  | "fido2"

interface SelectOption {
  value: string
  label: string
  description?: string | null
}

interface Prompt {
  key: string
  input_type: InputType
  label: string
  help?: string | null
  options?: SelectOption[]
  /// SecretConfirm prompts use this for the confirmation field
  /// label; the modal renders two inputs that must match.
  confirm_label?: string | null
}

interface CeremonyResponse {
  session_id: string
  prompts: Prompt[]
  messages: Message[]
  complete: boolean
  error?: string | null
  result_data?: Record<string, unknown> | null
}

// ── Component ─────────────────────────────────────────────────────

export type CeremonyKind = "init" | "join" | "invite" | "unlock"

const CEREMONY_TITLES: Record<CeremonyKind, string> = {
  init: "Place keystone — initialise pond",
  join: "Join pond",
  invite: "Open enrollment",
  unlock: "Unlock pond",
}

interface CeremonyModalProps {
  kind: CeremonyKind
  onClose: () => void
}

export function CeremonyModal({ kind, onClose }: CeremonyModalProps): JSX.Element {
  const [response, setResponse] = useState<CeremonyResponse | null>(null)
  const [busy, setBusy] = useState<boolean>(false)
  const [transportError, setTransportError] = useState<string | null>(null)
  const [inputs, setInputs] = useState<Record<string, string>>({})
  const [confirmInputs, setConfirmInputs] = useState<Record<string, string>>({})
  const firstInputRef = useRef<HTMLInputElement | null>(null)

  // Kick off the ceremony on mount.
  useEffect(() => {
    let cancelled = false
    void (async () => {
      setBusy(true)
      try {
        const initial = await invoke<CeremonyResponse>("ceremony_step", {
          request: {
            session_id: null,
            ceremony: kind,
            data: {},
          },
        })
        if (cancelled) return
        setResponse(initial)
        setTransportError(null)
      } catch (e) {
        if (!cancelled) setTransportError(String(e))
      } finally {
        if (!cancelled) setBusy(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [kind])

  // Reset per-step input state whenever a new response arrives.
  useEffect(() => {
    setInputs({})
    setConfirmInputs({})
    // Focus the first input on each step so the modal feels
    // keyboard-driven.
    setTimeout(() => firstInputRef.current?.focus(), 0)
  }, [response?.session_id, response?.prompts.length])

  const submit = useCallback(async () => {
    if (!response) return
    // Confirm inputs must match for SecretConfirm prompts.
    for (const prompt of response.prompts) {
      if (prompt.input_type === "secret_confirm") {
        const a = inputs[prompt.key] ?? ""
        const b = confirmInputs[prompt.key] ?? ""
        if (a !== b) {
          setTransportError(`'${prompt.label}' values do not match`)
          return
        }
      }
    }
    setBusy(true)
    setTransportError(null)
    try {
      const next = await invoke<CeremonyResponse>("ceremony_step", {
        request: {
          session_id: response.session_id,
          ceremony: null,
          data: inputs,
        },
      })
      setResponse(next)
    } catch (e) {
      setTransportError(String(e))
    } finally {
      setBusy(false)
    }
  }, [confirmInputs, inputs, response])

  const onKey = useCallback(
    (e: ReactKeyboardEvent<HTMLDivElement>) => {
      if (e.key === "Escape" && response?.complete) {
        e.preventDefault()
        onClose()
      }
      // Enter submits when there's at least one prompt and the
      // user isn't in a textarea/multi-line context.
      if (
        e.key === "Enter" &&
        !e.shiftKey &&
        response &&
        response.prompts.length > 0 &&
        !busy
      ) {
        e.preventDefault()
        void submit()
      }
    },
    [busy, onClose, response, submit]
  )

  const title = CEREMONY_TITLES[kind] ?? `Ceremony: ${kind}`

  return (
    <div
      className="ceremony-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onClick={(e) => {
        // Allow clicking the backdrop to close only when the
        // ceremony is complete or hasn't started yet — clicking
        // away mid-ceremony would lose the session.
        if (e.target === e.currentTarget && (!response || response.complete)) {
          onClose()
        }
      }}
    >
      <div className="ceremony" onKeyDown={onKey}>
        <header className="ceremony-head">
          <span className="ceremony-mark">P</span>
          <span className="ceremony-title">{title}</span>
          {response?.complete ? (
            <button
              type="button"
              className="ceremony-close"
              onClick={onClose}
              aria-label="Close"
            >
              ✕
            </button>
          ) : (
            <span className="ceremony-step-indicator">
              {busy ? "working…" : "in progress"}
            </span>
          )}
        </header>

        {transportError && (
          <div className="ceremony-error" role="alert">
            {transportError}
          </div>
        )}

        {!response ? (
          <div className="ceremony-loading">Starting ceremony…</div>
        ) : (
          <>
            {response.messages.map((m, i) => (
              <CeremonyMessage key={`${response.session_id}-msg-${i}`} message={m} />
            ))}

            {response.error && !response.complete && (
              <div className="ceremony-error" role="alert">
                {response.error}
              </div>
            )}

            {!response.complete && response.prompts.length > 0 && (
              <PromptsBlock
                prompts={response.prompts}
                inputs={inputs}
                onInput={(key, v) => setInputs((s) => ({ ...s, [key]: v }))}
                confirmInputs={confirmInputs}
                onConfirmInput={(key, v) =>
                  setConfirmInputs((s) => ({ ...s, [key]: v }))
                }
                firstInputRef={firstInputRef}
                disabled={busy}
              />
            )}
          </>
        )}

        <footer className="ceremony-foot">
          {response?.complete ? (
            <>
              <span className="ceremony-foot-status">
                {response.error ? "Failed" : "Complete"}
              </span>
              <button
                type="button"
                className="ceremony-primary"
                onClick={onClose}
              >
                Done
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                className="ceremony-secondary"
                onClick={onClose}
                disabled={busy}
              >
                Cancel
              </button>
              {response && response.prompts.length > 0 && (
                <button
                  type="button"
                  className="ceremony-primary"
                  onClick={submit}
                  disabled={busy}
                >
                  {busy ? "Submitting…" : "Continue"}
                </button>
              )}
            </>
          )}
        </footer>
      </div>
    </div>
  )
}

// ── Message rendering ─────────────────────────────────────────────

function CeremonyMessage({ message }: { message: Message }): JSX.Element {
  switch (message.kind) {
    case "qr_code":
      return <CeremonyQr message={message} />
    case "summary":
      return (
        <section className="ceremony-msg ceremony-msg-summary">
          <div className="ceremony-msg-title">{message.title}</div>
          <pre className="ceremony-msg-content">{message.content}</pre>
        </section>
      )
    case "error":
      return (
        <section className="ceremony-msg ceremony-msg-error">
          <div className="ceremony-msg-title">{message.title}</div>
          <div className="ceremony-msg-content">{message.content}</div>
        </section>
      )
    case "info":
    default:
      return (
        <section className="ceremony-msg ceremony-msg-info">
          {message.title && (
            <div className="ceremony-msg-title">{message.title}</div>
          )}
          <div className="ceremony-msg-content">{message.content}</div>
        </section>
      )
  }
}

function CeremonyQr({ message }: { message: Message }): JSX.Element {
  const trimmed = message.content.trim()
  const isPng = /^[A-Za-z0-9+/=\s]+$/.test(trimmed) && trimmed.length > 100
  const isUri = trimmed.startsWith("otpauth://") || trimmed.startsWith("http")
  return (
    <section className="ceremony-msg ceremony-msg-qr">
      {message.title && (
        <div className="ceremony-msg-title">{message.title}</div>
      )}
      {isPng ? (
        <img
          className="ceremony-qr-img"
          src={`data:image/png;base64,${trimmed}`}
          alt="QR code"
        />
      ) : isUri ? (
        <code className="ceremony-qr-uri">{trimmed}</code>
      ) : (
        <pre className="ceremony-qr-pre">{message.content}</pre>
      )}
    </section>
  )
}

// ── Prompts ───────────────────────────────────────────────────────

interface PromptsBlockProps {
  prompts: Prompt[]
  inputs: Record<string, string>
  onInput: (key: string, value: string) => void
  confirmInputs: Record<string, string>
  onConfirmInput: (key: string, value: string) => void
  firstInputRef: React.RefObject<HTMLInputElement | null>
  disabled: boolean
}

function PromptsBlock({
  prompts,
  inputs,
  onInput,
  confirmInputs,
  onConfirmInput,
  firstInputRef,
  disabled,
}: PromptsBlockProps): JSX.Element {
  return (
    <section className="ceremony-prompts">
      {prompts.map((prompt, i) => (
        <PromptRow
          key={prompt.key}
          prompt={prompt}
          value={inputs[prompt.key] ?? ""}
          onChange={(v) => onInput(prompt.key, v)}
          confirmValue={confirmInputs[prompt.key] ?? ""}
          onConfirmChange={(v) => onConfirmInput(prompt.key, v)}
          inputRef={i === 0 ? firstInputRef : null}
          disabled={disabled}
        />
      ))}
    </section>
  )
}

interface PromptRowProps {
  prompt: Prompt
  value: string
  onChange: (v: string) => void
  confirmValue: string
  onConfirmChange: (v: string) => void
  inputRef: React.RefObject<HTMLInputElement | null> | null
  disabled: boolean
}

function PromptRow({
  prompt,
  value,
  onChange,
  confirmValue,
  onConfirmChange,
  inputRef,
  disabled,
}: PromptRowProps): JSX.Element {
  const inputType = useMemo(() => mapInputType(prompt.input_type), [prompt.input_type])

  if (prompt.input_type === "select_one") {
    return (
      <div className="ceremony-prompt">
        <label className="ceremony-prompt-label">{prompt.label}</label>
        {prompt.help && (
          <div className="ceremony-prompt-help">{prompt.help}</div>
        )}
        <div className="ceremony-prompt-options">
          {(prompt.options ?? []).map((opt) => (
            <label className="ceremony-option" key={opt.value}>
              <input
                type="radio"
                name={prompt.key}
                value={opt.value}
                checked={value === opt.value}
                onChange={(e) => onChange(e.target.value)}
                disabled={disabled}
              />
              <span className="ceremony-option-label">
                <span>{opt.label}</span>
                {opt.description && (
                  <span className="ceremony-option-desc">{opt.description}</span>
                )}
              </span>
            </label>
          ))}
        </div>
      </div>
    )
  }

  return (
    <div className="ceremony-prompt">
      <label className="ceremony-prompt-label" htmlFor={`prompt-${prompt.key}`}>
        {prompt.label}
      </label>
      {prompt.help && (
        <div className="ceremony-prompt-help">{prompt.help}</div>
      )}
      <input
        id={`prompt-${prompt.key}`}
        ref={inputRef}
        type={inputType}
        className="ceremony-prompt-input"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        autoComplete={inputType === "password" ? "new-password" : "off"}
        spellCheck={false}
        disabled={disabled}
      />
      {prompt.input_type === "secret_confirm" && (
        <>
          <label
            className="ceremony-prompt-label"
            htmlFor={`prompt-${prompt.key}-confirm`}
          >
            {prompt.confirm_label ?? "Confirm"}
          </label>
          <input
            id={`prompt-${prompt.key}-confirm`}
            type="password"
            className="ceremony-prompt-input"
            value={confirmValue}
            onChange={(e) => onConfirmChange(e.target.value)}
            autoComplete="new-password"
            spellCheck={false}
            disabled={disabled}
          />
        </>
      )}
    </div>
  )
}

function mapInputType(input: InputType): string {
  switch (input) {
    case "secret":
    case "secret_confirm":
      return "password"
    case "code":
    case "entropy":
      return "text"
    case "text":
    default:
      return "text"
  }
}
