import { useTranslation } from "react-i18next";

interface WhatsNewModalProps {
  version: string;
  changelog: string;
  onDismiss: () => void;
}

/**
 * Parse the full CHANGELOG.md content and extract the section
 * for the given version. Returns the raw markdown lines (without
 * the version header).
 */
function extractVersionChangelog(changelog: string, version: string): string {
  const lines = changelog.split("\n");
  const versionClean = version.replace(/^v/, "");
  const startIdx = lines.findIndex((line) =>
    line.match(new RegExp(`^##\\s+\\[${versionClean.replace(/\./g, "\\.")}\\]`))
  );
  if (startIdx === -1) return "";

  const endIdx = lines.findIndex(
    (line, i) => i > startIdx && line.match(/^##\s+\[/)
  );
  const sectionLines = lines.slice(startIdx + 1, endIdx === -1 ? undefined : endIdx);

  return sectionLines.join("\n").trim();
}

/**
 * Extract bullets under a `### Highlights` heading within a version section.
 * Returns the bullet text array if the heading is present with at least one
 * bullet; null otherwise (absent, present-but-empty, or no bullet lines).
 */
function extractHighlights(sectionMarkdown: string): string[] | null {
  const lines = sectionMarkdown.split("\n");
  const startIdx = lines.findIndex((line) => line.trim() === "### Highlights");
  if (startIdx === -1) return null;

  const endIdx = lines.findIndex(
    (line, i) => i > startIdx && /^#{2,3}\s/.test(line)
  );
  const bodyLines = lines.slice(startIdx + 1, endIdx === -1 ? undefined : endIdx);

  const bullets = bodyLines
    .map((line) => line.trim())
    .filter((line) => line.startsWith("- "))
    .map((line) => line.slice(2));

  return bullets.length > 0 ? bullets : null;
}

function ChangelogContent({ markdown }: { markdown: string }) {
  const lines = markdown.split("\n");
  const elements: React.ReactNode[] = [];
  let key = 0;

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;

    if (trimmed.startsWith("### ")) {
      elements.push(
        <h3 key={key++} className="text-sm font-semibold text-primary mt-3 mb-1.5">
          {trimmed.slice(4)}
        </h3>
      );
    } else if (trimmed.startsWith("- ")) {
      elements.push(
        <li key={key++} className="text-sm text-foreground/80 ml-4 mb-1 list-disc">
          {trimmed.slice(2)}
        </li>
      );
    } else {
      elements.push(
        <p key={key++} className="text-sm text-foreground/80 mb-1">
          {trimmed}
        </p>
      );
    }
  }

  return <div className="space-y-0">{elements}</div>;
}

export default function WhatsNewModal({ version, changelog, onDismiss }: WhatsNewModalProps) {
  const { t } = useTranslation();
  const sectionMarkdown = extractVersionChangelog(changelog, version);
  const highlights = extractHighlights(sectionMarkdown);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-background border border-border rounded-xl shadow-2xl w-full max-w-lg mx-4 max-h-[70vh] flex flex-col">
        <div className="px-5 pt-5 pb-3 border-b border-border-subtle shrink-0">
          <h2 className="text-base font-semibold text-foreground">
            {t("whatsNew.title", { version: version.replace(/^v/, "") })}
          </h2>
        </div>

        <div className="px-5 py-4 overflow-y-auto flex-1">
          {highlights ? (
            <ul className="space-y-1.5">
              {highlights.map((bullet, i) => (
                <li key={i} className="text-sm text-foreground/80 ml-4 list-disc">
                  {bullet}
                </li>
              ))}
            </ul>
          ) : sectionMarkdown ? (
            <ChangelogContent markdown={sectionMarkdown} />
          ) : (
            <p className="text-sm text-muted-foreground">{t("whatsNew.noChangelog")}</p>
          )}
        </div>

        <div className="px-5 py-3 border-t border-border-subtle shrink-0 flex justify-end">
          <button
            onClick={onDismiss}
            className="px-4 py-1.5 text-sm font-medium rounded-control bg-primary/15 text-primary border border-primary/30 hover:bg-primary/25 transition-colors"
          >
            {t("whatsNew.dismiss")}
          </button>
        </div>
      </div>
    </div>
  );
}
