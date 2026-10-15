// Pixel flame logo (same art as the app icon).
const FLAME = [
  "......#.....", ".....##.....", "....###..#..", "....####.##.", "...#####.##.",
  "..##oo####..", "..#ooo#####.", ".##oyyo####.", ".#ooyyyoo###", ".#oyyyyyoo##",
  ".#oyyWWyyo##", "..#oyWWyyo#.", "..##oyyyo##.", "....######..",
];
const COLORS = { "#": "#e8481c", o: "#ff8a24", y: "#ffcc40", W: "#fff4c8" };
const svg = document.getElementById("flame");
FLAME.forEach((row, y) => [...row].forEach((c, x) => {
  if (!COLORS[c]) return;
  const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
  for (const [k, v] of Object.entries({ x, y, width: 1, height: 1, fill: COLORS[c] })) rect.setAttribute(k, v);
  svg.append(rect);
}));

// Point the main button at the visitor's platform.
const platform = (navigator.userAgentData?.platform || navigator.platform || navigator.userAgent).toLowerCase();
const os = platform.includes("win") ? "win" : platform.includes("mac") ? "mac" : null;
if (os) {
  const card = document.getElementById(`card-${os}`);
  const link = card.querySelector("a.button");
  const hero = document.getElementById("hero-download");
  hero.href = link.getAttribute("href");
  document.getElementById("hero-label").textContent = os === "mac" ? "Download for macOS" : "Download for Windows";
  document.getElementById("hero-icon").className = `os-icon ${os}`;
  card.classList.add("is-detected");
  // Windows on ARM gets the native ARM64 build.
  if (os === "win") navigator.userAgentData?.getHighEntropyValues(["architecture"])
    .then(({ architecture }) => { if (architecture === "arm") hero.href = document.getElementById("win-arm64").href; })
    .catch(() => {});
  // After downloading, show the install steps.
  hero.addEventListener("click", () => setTimeout(() => card.scrollIntoView({ behavior: "smooth", block: "center" }), 300));
}

// Click a command to copy it.
for (const el of document.querySelectorAll(".copy")) {
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(el.textContent.trim());
      el.classList.add("is-copied");
      setTimeout(() => el.classList.remove("is-copied"), 1200);
    } catch {}
  };
  el.addEventListener("click", copy);
  el.addEventListener("keydown", (e) => { if (e.key === "Enter") copy(); });
}
