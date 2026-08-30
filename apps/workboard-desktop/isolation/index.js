const allowedCommands = new Set([
  "workboard_handshake",
  "workboard_query",
  "workboard_execute",
  "workboard_subscribe",
]);

window.__TAURI_ISOLATION_HOOK__ = (payload) => {
  if (typeof payload !== "object" || payload === null || !allowedCommands.has(payload.cmd)) {
    throw new Error("IPC command denied");
  }

  return payload;
};
