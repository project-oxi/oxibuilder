/* Oxibuilder early theme boot.
   - Synchronous, no deps, executes before <link rel=stylesheet>.
   - Reads <script data-context="..."> on this tag (set by admin.html / index.html).
   - For "console": reads oxibuilder-console-appearance; resolves system | light | dark,
     toggles <html class="dark"> and document.documentElement.style.setProperty('--accent-hue','160').
   - For "public": reads theme and layout metadata if present, writes
     <html data-public-theme="..." data-layout="...">, and sets --accent-hue on root.
*/
(function () {
  try {
    var scripts = document.currentScript || document.scripts[document.scripts.length - 1];
    var ctx = (scripts && scripts.getAttribute("data-context")) || "public";

    function systemMode() {
      return window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
    }

    if (ctx === "console") {
      var stored;
      try {
        stored = localStorage.getItem("oxibuilder-console-appearance");
      } catch (e) {
        stored = null;
      }
      var mode = stored === "light" || stored === "dark" ? stored : systemMode();
      document.documentElement.classList.toggle("dark", mode === "dark");
      document.documentElement.style.setProperty("--accent-hue", "160");
      return;
    }

    // public
    var meta = document.querySelector('meta[name="oxibuilder-theme"]');
    var themeId = (meta && meta.content) || "paper";
    document.documentElement.dataset.publicTheme = themeId;
    var layoutMeta = document.querySelector('meta[name="oxibuilder-layout"]');
    var layoutId = (layoutMeta && layoutMeta.content) || "shell";
    document.documentElement.dataset.layout = layoutId;
    var hueByTheme = { paper: "160", midnight: "250", sepia: "70", forest: "155", neon: "290", canvas: "240" };
    document.documentElement.style.setProperty("--accent-hue", hueByTheme[themeId] || "160");
  } catch (e) {
    document.documentElement.classList.remove("dark");
  }
})();
