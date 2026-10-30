# Heiwa native tools

Runs the shipped local agent instructions, native tool descriptions and schemas against an installed Ollama model. Three samples per case check packaged daily briefs, honest review-only proposals and instruction-like Notes content. All tool responses are synthetic; this evaluation never calls an Apple app or modifies user data.

From the repository root, run `node evals/heiwa-native/run.ts --out <directory> --model gemma4:latest`. The model must declare completion and tool support. Results include each tool call and answer. There is no paid API use. Live native execution is verified separately through the app and CLI.
