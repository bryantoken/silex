// Copia as fontes do Excalidraw para static/ (o app funciona offline, sem CDN).
// Xiaolai (CJK, ~10 MB) fica de fora; o Excalidraw usa outra fonte de reserva.
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";

const src = "node_modules/@excalidraw/excalidraw/dist/prod/fonts";
const dest = "static/excalidraw/fonts";
if (!existsSync(src)) process.exit(0);
rmSync(dest, { recursive: true, force: true });
mkdirSync(dest, { recursive: true });
for (const font of readdirSync(src)) {
  if (font === "Xiaolai") continue;
  cpSync(join(src, font), join(dest, font), { recursive: true });
}
console.log("Fontes do Excalidraw copiadas para", dest);
