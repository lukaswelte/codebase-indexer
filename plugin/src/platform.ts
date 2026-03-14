import process from "node:process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export function getBinaryPath(): string {
  const platform = process.platform;
  const arch = process.arch;

  let binaryName = "codebase-indexer";

  if (platform === "darwin") {
    if (arch === "arm64") {
      binaryName = "codebase-indexer-macos-arm64";
    } else {
      binaryName = "codebase-indexer-macos-x64";
    }
  } else if (platform === "linux") {
    binaryName = "codebase-indexer-linux-x64";
  }

  // Resolving relative to dist/platform.js
  return path.resolve(__dirname, "..", "bin", binaryName);
}
