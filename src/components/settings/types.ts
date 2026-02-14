import type { Settings } from "@/hooks/useSettings";

export interface SectionProps {
  settings: Settings;
  update: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  toast?: (props: {
    title?: string;
    description?: string;
    variant: "default" | "destructive" | "success";
  }) => void;
}
