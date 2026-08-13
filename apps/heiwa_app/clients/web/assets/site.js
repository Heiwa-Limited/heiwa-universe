(function () {
  const page = document.body?.dataset?.page;
  document.querySelectorAll(".nav a").forEach((a) => {
    const href = a.getAttribute("href") || "";
    if ((page === "home" && href.includes("index.html")) || href.includes(`${page}.html`)) {
      a.classList.add("is-active");
    }
  });

  const rates = {
    CAD: { mult: 1.0, sym: "CAD", decimals: 2 },
    USD: { mult: 0.73, sym: "USD", decimals: 2 },
    EUR: { mult: 0.67, sym: "EUR", decimals: 2 },
    GBP: { mult: 0.58, sym: "GBP", decimals: 2 },
    JPY: { mult: 105.0, sym: "JPY", decimals: 0 },
  };
  const currencySelect = document.getElementById("ccy-select");
  const applyCurrency = (code) => {
    const rate = code === "OFF" ? null : rates[code];
    document.querySelectorAll(".cost-num[data-cost-base-cad]").forEach((element) => {
      const base = Number.parseFloat(element.dataset.costBaseCad || "0");
      element.textContent = rate ? (base * rate.mult).toFixed(rate.decimals) : "—";
    });
    document.querySelectorAll(".ccy-suffix").forEach((element) => {
      element.textContent = rate ? rate.sym : "";
    });
  };
  if (currencySelect) {
    const stored = localStorage.getItem("heiwa.ccy");
    if (stored && (rates[stored] || stored === "OFF")) {
      currencySelect.value = stored;
      applyCurrency(stored);
    }
    currencySelect.addEventListener("change", () => {
      localStorage.setItem("heiwa.ccy", currencySelect.value);
      applyCurrency(currencySelect.value);
    });
  }

  document.querySelectorAll("[data-copy]").forEach((button) => {
    button.addEventListener("click", async () => {
      const target = document.getElementById(button.dataset.copy || "");
      if (!target) return;
      try {
        await navigator.clipboard.writeText((target.textContent || "").trim());
        const previous = button.textContent;
        button.textContent = "Copied";
        button.style.color = "var(--active)";
        button.style.borderColor = "var(--active)";
        setTimeout(() => {
          button.textContent = previous;
          button.style.color = "";
          button.style.borderColor = "";
        }, 1200);
      } catch {
        button.textContent = "Copy failed";
        setTimeout(() => {
          button.textContent = "Copy";
        }, 1200);
      }
    });
  });

  document.querySelectorAll(".ig-btn").forEach((button) => {
    button.addEventListener("click", () => {
      const colours = {
        active: "var(--active)",
        system: "var(--system)",
        critical: "var(--critical)",
      };
      const colour = colours[button.dataset.intent] || "var(--ink-2)";
      const previous = [button.style.background, button.style.color, button.style.borderColor];
      button.style.background = colour;
      button.style.color = "var(--bg)";
      button.style.borderColor = colour;
      setTimeout(() => {
        [button.style.background, button.style.color, button.style.borderColor] = previous;
      }, 80);
    });
  });
})();
