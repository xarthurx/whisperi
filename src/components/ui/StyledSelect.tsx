import React, { useState, useRef, useEffect } from "react";
import { createPortal } from "react-dom";
import { ChevronDown, Check } from "lucide-react";

export interface SelectOption {
  value: string;
  label: string;
}

interface StyledSelectProps {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  className?: string;
  placeholder?: string;
}

export default function StyledSelect({
  value,
  onChange,
  options,
  className = "",
  placeholder,
}: StyledSelectProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [highlightedIndex, setHighlightedIndex] = useState(0);
  const [dropdownPosition, setDropdownPosition] = useState({ top: 0, left: 0, width: 0 });
  const containerRef = useRef<HTMLDivElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  const portalTarget = useRef<HTMLElement>(document.body);

  useEffect(() => {
    if (containerRef.current) {
      const dialog = containerRef.current.closest('[role="dialog"]');
      portalTarget.current = (dialog as HTMLElement) ?? document.body;
    }
  }, []);

  useEffect(() => {
    if (isOpen && triggerRef.current) {
      const triggerRect = triggerRef.current.getBoundingClientRect();
      const target = portalTarget.current;
      const offsetX = target === document.body ? 0 : target.getBoundingClientRect().left;
      const offsetY = target === document.body ? 0 : target.getBoundingClientRect().top;
      setDropdownPosition({
        top: triggerRect.bottom + 4 - offsetY,
        left: triggerRect.left - offsetX,
        width: triggerRect.width,
      });
      // Set highlighted to current value
      const idx = options.findIndex((o) => o.value === value);
      setHighlightedIndex(idx >= 0 ? idx : 0);
    }
  }, [isOpen, options, value]);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      const target = event.target as Node;
      if (
        containerRef.current &&
        !containerRef.current.contains(target) &&
        (!dropdownRef.current || !dropdownRef.current.contains(target))
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!isOpen) {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
        e.preventDefault();
        setIsOpen(true);
      }
      return;
    }

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setHighlightedIndex((prev) => (prev < options.length - 1 ? prev + 1 : 0));
        break;
      case "ArrowUp":
        e.preventDefault();
        setHighlightedIndex((prev) => (prev > 0 ? prev - 1 : options.length - 1));
        break;
      case "Enter":
        e.preventDefault();
        if (options[highlightedIndex]) {
          handleSelect(options[highlightedIndex].value);
        }
        break;
      case "Escape":
        e.preventDefault();
        setIsOpen(false);
        break;
    }
  };

  const handleSelect = (optionValue: string) => {
    onChange(optionValue);
    setIsOpen(false);
  };

  const selectedLabel = options.find((o) => o.value === value)?.label ?? placeholder ?? value;

  return (
    <div className={`relative ${className}`} ref={containerRef}>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        onKeyDown={handleKeyDown}
        className={`group relative w-full flex items-center justify-between gap-2 h-9 px-2.5 text-left rounded-control text-sm font-medium border shadow-sm transition-all duration-200 ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 focus-visible:ring-offset-1 ${
          isOpen
            ? "border-border-active bg-surface-2/90 shadow ring-1 ring-primary/20"
            : "border-border/70 bg-surface-1/80 hover:border-border-hover hover:bg-surface-2/70 hover:shadow active:scale-[0.985]"
        }`}
        aria-haspopup="listbox"
        aria-expanded={isOpen}
      >
        <span className="truncate text-foreground">{selectedLabel}</span>
        <ChevronDown
          className={`w-3.5 h-3.5 shrink-0 text-muted-foreground transition-all duration-200 ${
            isOpen ? "rotate-180 text-primary" : "group-hover:text-foreground"
          }`}
        />
      </button>

      {isOpen &&
        createPortal(
          <div
            ref={dropdownRef}
            style={{
              position: "fixed",
              top: `${dropdownPosition.top}px`,
              left: `${dropdownPosition.left}px`,
              width: `${dropdownPosition.width}px`,
            }}
            className="z-[9999] bg-surface-1 backdrop-blur-xl border border-border rounded-control shadow-xl overflow-hidden"
          >
            <div className="max-h-48 overflow-y-auto px-1 py-1">
              <div role="listbox" className="space-y-0.5">
                {options.map((option, index) => {
                  const isSelected = option.value === value;
                  const isHighlighted = index === highlightedIndex;
                  return (
                    <button
                      key={option.value}
                      type="button"
                      onClick={() => handleSelect(option.value)}
                      className={`group w-full flex items-center justify-between gap-2 h-8 px-2.5 text-left text-sm font-medium rounded-inner transition-all duration-150 ease-out ${
                        isSelected
                          ? "bg-primary/10 text-primary shadow-sm"
                          : isHighlighted
                            ? "bg-surface-2 text-foreground"
                            : "text-foreground hover:bg-surface-2 active:scale-[0.98]"
                      }`}
                      role="option"
                      aria-selected={isSelected}
                    >
                      <span className="truncate">{option.label}</span>
                      {isSelected && <Check className="w-3 h-3 shrink-0" />}
                    </button>
                  );
                })}
              </div>
            </div>
          </div>,
          portalTarget.current
        )}
    </div>
  );
}
