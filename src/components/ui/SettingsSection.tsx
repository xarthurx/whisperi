import React from "react";

interface SettingsSectionProps {
  title: React.ReactNode;
  description?: string;
  children?: React.ReactNode;
  className?: string;
}

export const SettingsSection: React.FC<SettingsSectionProps> = ({
  title,
  description,
  children,
  className = "",
}) => {
  return (
    <div className={`space-y-2.5 ${className}`}>
      <div>
        <h3 className="text-base font-semibold text-foreground-bright tracking-tight">{title}</h3>
        {description && (
          <p className="text-[13px] text-muted-foreground mt-0.5 leading-relaxed">
            {description}
          </p>
        )}
      </div>
      <div className="pl-3 space-y-2.5">{children}</div>
    </div>
  );
};

interface SettingsRowProps {
  label: string;
  description?: string;
  children: React.ReactNode;
  className?: string;
}

export const SettingsRow: React.FC<SettingsRowProps> = ({
  label,
  description,
  children,
  className = "",
}) => {
  return (
    <div className={`flex items-center justify-between gap-4 ${className}`}>
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-foreground">{label}</p>
        {description && (
          <p className="text-xs text-muted-foreground/80 mt-0.5 leading-relaxed">
            {description}
          </p>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
};
