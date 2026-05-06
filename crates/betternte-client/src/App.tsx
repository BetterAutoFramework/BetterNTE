import { useEffect, useState } from "react";
import { BrowserRouter, Navigate,Route, Routes } from "react-router-dom";

import { DebugPanel } from "@/components/DebugPanel";
import { FloatingLogLayer } from "@/components/FloatingLogLayer";
import { ErrorDialog } from "@/components/ErrorDialog";
import { Sidebar } from "@/components/layout/Sidebar";
import { StatusBar } from "@/components/layout/StatusBar";
import { TitleBar } from "@/components/layout/TitleBar";
import { LogDrawer } from "@/components/LogDrawer";
import { FlowEditorPage } from "@/pages/FlowEditorPage";
import { HomePage } from "@/pages/HomePage";
import { InputTestPage } from "@/pages/InputTestPage";
import { OneDragonFlow } from "@/pages/OneDragonFlow";
import { ScriptDebugPage } from "@/pages/ScriptDebugPage";
import { Settings } from "@/pages/Settings";
import { TaskPage } from "@/pages/TaskPage";
import { TriggerPage } from "@/pages/TriggerPage";

const DEV_MODE_KEY = "betternte-developer-mode";

export default function App() {
  const [devMode, setDevMode] = useState(() => localStorage.getItem(DEV_MODE_KEY) === "true");

  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      setDevMode(Boolean(detail));
    };
    window.addEventListener("developer-mode-changed", handler);
    return () => window.removeEventListener("developer-mode-changed", handler);
  }, []);

  return (
    <BrowserRouter>
      <div className="flex flex-col h-screen bg-background">
        <TitleBar />
        <div className="flex flex-1 overflow-hidden">
          <Sidebar />
          <main className="flex-1 overflow-y-auto transition-[width] duration-200">
            <Routes>
              <Route path="/" element={<HomePage />} />
              <Route path="/triggers" element={<TriggerPage />} />
              <Route path="/scripts" element={<TaskPage />} />
              <Route path="/one-dragon" element={<OneDragonFlow />} />
              <Route
                path="/input-test"
                element={devMode ? <InputTestPage /> : <Navigate to="/" replace />}
              />
              <Route path="/workflow" element={<FlowEditorPage />} />
              <Route path="/debug" element={<ScriptDebugPage />} />
              <Route path="/tasks" element={<Navigate to="/scripts" replace />} />
              <Route path="/settings" element={<Settings />} />
            </Routes>
          </main>
        </div>
        <StatusBar />
        <FloatingLogLayer />
        <LogDrawer />
        <DebugPanel />
        <ErrorDialog />
      </div>
    </BrowserRouter>
  );
}
