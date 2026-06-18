import React from "react";
import ReactDOM from "react-dom/client";
import { ReactFlowProvider } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import "./styles/index.css";
import { App } from "./App";

function showBootError(error: unknown) {
  const root = document.getElementById("root");
  const message = error instanceof Error ? `${error.name}: ${error.message}` : String(error);
  if (root) {
    root.innerHTML = `
      <main style="background:#030303;color:#f1a4a4;font-family:system-ui,sans-serif;min-height:100vh;padding:18px">
        <h1 style="font-size:18px;margin:0 0 12px">VirtualAudioMix n'a pas pu démarrer l'interface.</h1>
        <pre style="white-space:pre-wrap;background:#190909;border:1px solid #502424;border-radius:8px;padding:12px">${message}</pre>
      </main>
    `;
  }
}

window.addEventListener("error", (event) => showBootError(event.error ?? event.message));
window.addEventListener("unhandledrejection", (event) => showBootError(event.reason));

try {
  const root = document.getElementById("root");
  if (!root) {
    throw new Error("Element #root introuvable.");
  }

  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <ReactFlowProvider>
        <App />
      </ReactFlowProvider>
    </React.StrictMode>,
  );
} catch (error) {
  showBootError(error);
}
