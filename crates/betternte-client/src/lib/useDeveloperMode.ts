import { useEffect, useState } from "react";

const DEV_MODE_KEY = "betternte-developer-mode";

/** Tracks Settings → 开发者模式 (localStorage + `developer-mode-changed` event). */
export function useDeveloperMode(): boolean {
  const [on, setOn] = useState(() => localStorage.getItem(DEV_MODE_KEY) === "true");

  useEffect(() => {
    const handler = (e: Event) => {
      setOn(Boolean((e as CustomEvent<boolean>).detail));
    };
    window.addEventListener("developer-mode-changed", handler);
    return () => window.removeEventListener("developer-mode-changed", handler);
  }, []);

  return on;
}
