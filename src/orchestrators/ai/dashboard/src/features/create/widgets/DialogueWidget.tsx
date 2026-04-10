import { useRef, useEffect } from "react";

interface Turn {
  user: string;
  assistant: string;
}

interface Props {
  value: Turn[];
  streamingText?: string;
  onChange: (value: Turn[]) => void;
}

/**
 * Conversation thread widget for fields of type "dialogue".
 * Renders alternating user/assistant message bubbles. The form
 * drives this — the widget just displays turns and doesn't own
 * dispatch logic.
 */
export default function DialogueWidget({ value, streamingText, onChange: _onChange }: Props) {
  const threadRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    threadRef.current?.scrollTo({ top: threadRef.current.scrollHeight, behavior: "smooth" });
  }, [value, streamingText]);

  if (value.length === 0 && !streamingText) {
    return (
      <div className="py-6 text-center text-text-dimmer text-xs italic">
        Start a conversation
      </div>
    );
  }

  return (
    <div ref={threadRef} className="space-y-2 max-h-[50vh] overflow-y-auto py-2">
      {value.map((turn, i) => (
        <div key={i}>
          <Bubble role="user" text={turn.user} />
          <Bubble role="assistant" text={turn.assistant} />
        </div>
      ))}
      {streamingText && (
        <Bubble role="assistant" text={streamingText} />
      )}
    </div>
  );
}

function Bubble({ role, text }: { role: "user" | "assistant"; text: string }) {
  const isUser = role === "user";
  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"} mb-1`}>
      <div
        className={[
          "max-w-[85%] px-3 py-2 rounded-lg text-[13px] leading-relaxed",
          isUser ? "bg-accent/15 text-text" : "bg-surface-2 text-text",
        ].join(" ")}
      >
        <div className="whitespace-pre-wrap">{text}</div>
      </div>
    </div>
  );
}
