import { ReactNode, useRef, useState, useEffect, useCallback } from "react";
import { ProviderIcon } from "./ProviderIcon";

export interface ProviderTabItem {
  id: string;
  name: string;
  recommended?: boolean;
  hasKey?: boolean;
}

interface ProviderTabsProps {
  providers: ProviderTabItem[];
  selectedId: string;
  onSelect: (id: string) => void;
  renderIcon?: (providerId: string) => ReactNode;
}

export function ProviderTabs({
  providers,
  selectedId,
  onSelect,
  renderIcon,
}: ProviderTabsProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [indicatorStyle, setIndicatorStyle] = useState<React.CSSProperties>({ opacity: 0 });

  const updateIndicator = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;

    const selectedIndex = providers.findIndex((p) => p.id === selectedId);
    if (selectedIndex === -1) {
      setIndicatorStyle({ opacity: 0 });
      return;
    }

    const buttons = container.querySelectorAll<HTMLButtonElement>("[data-tab-button]");
    const selectedButton = buttons[selectedIndex];
    if (!selectedButton) return;

    const containerRect = container.getBoundingClientRect();
    const buttonRect = selectedButton.getBoundingClientRect();

    setIndicatorStyle({
      width: buttonRect.width,
      height: buttonRect.height,
      transform: `translate(${buttonRect.left - containerRect.left}px, ${buttonRect.top - containerRect.top}px)`,
      opacity: 1,
    });
  }, [providers, selectedId]);

  useEffect(() => {
    updateIndicator();
  }, [updateIndicator]);

  useEffect(() => {
    const observer = new ResizeObserver(() => updateIndicator());
    if (containerRef.current) observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, [updateIndicator]);

  return (
    <div
      ref={containerRef}
      className="relative flex flex-wrap p-0.5 rounded-md bg-surface-1"
    >
      <div
        className="absolute top-0.5 left-0 rounded-md bg-primary/15 border border-primary/30 transition-all duration-200 ease-out pointer-events-none"
        style={indicatorStyle}
      />
      {providers.map((provider) => {
        const isSelected = selectedId === provider.id;
        return (
          <button
            key={provider.id}
            data-tab-button
            onClick={() => onSelect(provider.id)}
            className={`relative z-10 flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-md font-medium text-sm transition-all duration-150 border ${
              isSelected ? "text-primary font-semibold border-transparent" : "text-muted-foreground hover:text-foreground border-transparent"
            }`}
          >
            {renderIcon ? renderIcon(provider.id) : <ProviderIcon provider={provider.id} className="w-4 h-4" />}
            <span>{provider.name}</span>
            {provider.hasKey && (
              <span className="w-1.5 h-1.5 rounded-full bg-success shrink-0" title="API key configured" />
            )}
            {provider.recommended && (
              <span className="text-[11px] text-primary/70 font-medium">Recommended</span>
            )}
          </button>
        );
      })}
    </div>
  );
}
