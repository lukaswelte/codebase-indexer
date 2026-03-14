import { Plugin, tool } from "@opencode-ai/plugin";
import { getBinaryPath } from "./platform.js";

export const CodebaseIndexerPlugin: Plugin = async ({ project, $ }) => {
  const binary = getBinaryPath();

  // Start background watcher for the project root
  // The & in bun shell/opencode plugin $ creates a background process
  try {
    console.log(`Starting codebase-indexer watch for ${project.worktree}`);
    $`${binary} watch --root ${project.worktree} &`;
  } catch (error) {
    console.error("Failed to start codebase-indexer watcher:", error);
  }

  return {
    tool: {
      search: tool({
        description: "Search the codebase for relevant snippets using semantic vector embeddings. Use this for answering questions about the codebase or finding specific implementation details.",
        args: {
          query: tool.schema.string().describe("The search query to find relevant code snippets"),
        },
        async execute({ query }) {
          try {
            const { stdout, stderr, exitCode } = await $`${binary} search "${query}"`;
            
            if (exitCode !== 0) {
              return `Error searching codebase: ${stderr.toString()}`;
            }
            
            return stdout.toString();
          } catch (error: any) {
            return `Failed to execute search: ${error.message}`;
          }
        },
      }),
    },
  };
};

export default CodebaseIndexerPlugin;
