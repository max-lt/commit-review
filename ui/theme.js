// Light or dark: the reviewer's choice, kept in this browser, or the
// system's until a choice is made. Loaded in <head> so the first paint is
// already right; every [data-theme-switch] button names the other theme.
const theme = (() => {
  const KEY = "commit-review-theme";
  const media = matchMedia("(prefers-color-scheme: dark)");
  const stored = () => {
    try { return localStorage.getItem(KEY); } catch { return null; }
  };
  const apply = () => {
    const current = stored() || (media.matches ? "dark" : "light");
    document.documentElement.dataset.theme = current;
    document.querySelectorAll("[data-theme-switch]").forEach((b) => { b.textContent = current === "dark" ? "Light" : "Dark"; });
  };
  media.addEventListener("change", apply);
  document.addEventListener("DOMContentLoaded", apply);
  apply();
  return {
    toggle() {
      const next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
      try { localStorage.setItem(KEY, next); } catch {}
      apply();
    },
  };
})();
