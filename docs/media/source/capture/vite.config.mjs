// The app's dev server (sample data, no backend) with the sample data in
// English. From the repo root:
//   npx --prefix app vite --config docs/media/source/capture/vite.config.mjs
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { applyOverlay } from "./sample-en.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const app = resolve(here, "../../../../app");
const require = createRequire(resolve(app, "package.json"));
const { default: react } = await import(pathToFileURL(require.resolve("@vitejs/plugin-react")).href);

const sampleEnglish = {
  name: "hedy-sample-en",
  enforce: "pre",
  transform(code, id) {
    if (!/\/src\/api\/(MockApi|mockCodeSamples)\.ts$/.test(id)) return null;
    return { code: applyOverlay(id, code, (m) => this.warn(m)), map: null };
  },
};

export default {
  root: app,
  plugins: [sampleEnglish, react()],
  clearScreen: false,
  server: { port: Number(process.env.PORT ?? 5237), strictPort: true },
};
