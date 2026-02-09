import { Brain, Wrench } from "lucide-react";

interface ProviderIconProps {
  provider: string;
  className?: string;
}

// Simple provider icon component using Lucide fallbacks.
// Avoids dependency on external icon assets.
const PROVIDER_LABELS: Record<string, string> = {
  openai: "O",
  anthropic: "A",
  gemini: "G",
  groq: "Q",
  mistral: "M",
};

export function ProviderIcon({ provider, className = "w-5 h-5" }: ProviderIconProps) {
  if (provider === "custom") {
    return <Wrench className={className} />;
  }

  const label = PROVIDER_LABELS[provider];
  if (label) {
    return (
      <span
        className={`inline-flex items-center justify-center rounded bg-surface-raised text-[10px] font-bold text-foreground/70 ${className}`}
      >
        {label}
      </span>
    );
  }

  return <Brain className={className} />;
}
