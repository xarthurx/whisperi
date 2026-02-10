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
      transform: `translateX(${buttonRect.left - containerRect.left}px)`,
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
      className="relative flex p-0.5 rounded-md bg-surface-1"
    >
      <div
        className="absolute top-0.5 left-0 rounded-md bg-card border border-border-subtle shadow-sm transition-all duration-200 ease-out pointer-events-none"
        style={indicatorStyle}
      />
      {providers.map((provider) => {
        const isSelected = selectedId === provider.id;
        return (
          <button
            key={provider.id}
            data-tab-button
            onClick={() => onSelect(provider.id)}
            className={`relative z-10 flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-md font-medium text-xs transition-colors duration-150 ${
              isSelected ? "text-foreground" : "text-muted-foreground hover:text-foreground"
            }`}
          >
            {renderIcon ? renderIcon(provider.id) : <ProviderIcon provider={provider.id} className="w-4 h-4" />}
            <span>{provider.name}</span>
            {provider.hasKey && (
              <span className="w-1.5 h-1.5 rounded-full bg-success shrink-0" title="API key configured" />
            )}
            {provider.recommended && !provider.hasKey && (
              <span className="text-[9px] text-primary/70 font-medium">Recommended</span>
            )}
          </button>
        );
      })}
    </div>
  );
}
