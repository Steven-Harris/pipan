// Companion plugin for examples/channel-reference/producer.ts.
// The plugin id is the channel id, so `pipan.channel("acme.demo")` talks to that producer.
const ID = "acme.demo";

const stops = [];

window.pipan.registerExtension({
  id: ID,

  // A custom panel: the producer's "graph" topic summarized as counts, plus a button that sends an intent.
  panels: {
    "demo-status": (container) => {
      const channel = window.pipan.channel(ID);
      container.innerHTML = `
        <div style="padding: 1rem; font-family: sans-serif">
          <h3 style="margin: 0 0 .5rem">Demo status</h3>
          <p data-role="summary">Waiting for the producer.</p>
          <button data-role="refresh" type="button">Ask producer to refresh</button>
        </div>`;
      const summary = container.querySelector('[data-role="summary"]');
      container.querySelector('[data-role="refresh"]').addEventListener("click", () => {
        channel.intent("refresh", { from: "demo-status" });
      });

      // Called now with the latest value, then on every change. null means nothing is published.
      return channel.subscribe("graph", (snapshot) => {
        if (!snapshot) {
          summary.textContent = "Waiting for the producer.";
          return;
        }
        const counts = {};
        for (const node of snapshot.data.nodes ?? []) counts[node.status] = (counts[node.status] ?? 0) + 1;
        summary.textContent = Object.entries(counts).map(([status, n]) => `${n} ${status}`).join(", ") || "No nodes.";
      });
    },
  },

  // Draws the producer's log messages named "note" in the chat transcript.
  messageRenderers: {
    note: (message, container) => {
      const el = document.createElement("div");
      el.style.cssText = "border-left: 3px solid #6366f1; padding-left: .75rem";
      el.textContent = `Note: ${message.content ?? ""}`;
      container.append(el);
      return () => el.remove();
    },
  },

  // Takes over requests of kind "mystery": shows two buttons and answers with the chosen one.
  activate() {
    stops.push(
      window.pipan.channel(ID).onRequest("mystery", (container, request) => {
        container.innerHTML = `
          <p>The producer asked: <code data-role="payload"></code></p>
          <div style="display: flex; gap: .5rem; margin-top: .75rem">
            <button data-answer="yes" type="button">Yes</button>
            <button data-answer="no" type="button">No</button>
          </div>`;
        container.querySelector('[data-role="payload"]').textContent = JSON.stringify(request.payload);
        for (const button of container.querySelectorAll("button")) {
          button.addEventListener("click", () => request.answer({ choice: button.dataset.answer }));
        }
      }),
    );
  },

  deactivate() {
    while (stops.length) stops.pop()();
  },
});
