document.addEventListener("submit", (event) => {
  const form = event.target;
  const message = form && form.dataset ? form.dataset.confirm : "";
  if (message && !window.confirm(message)) {
    event.preventDefault();
  }
});

async function refreshPollTables() {
  for (const table of document.querySelectorAll("[data-poll]")) {
    try {
      const resp = await fetch(table.dataset.poll, {credentials: "same-origin"});
      if (!resp.ok) continue;
      table.dataset.lastRefresh = new Date().toISOString();
    } catch (_) {
      /* keep the rendered snapshot */
    }
  }
}

window.setInterval(refreshPollTables, 1000);
